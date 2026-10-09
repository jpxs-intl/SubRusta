use std::{
    collections::HashMap,
    net::SocketAddr,
    path::Path,
    time::{Duration, Instant},
};

use rosa_map::file_types::srk::SrkData;
use rosa_math::vector::Vector;
use rosa_protocol::{
    GameMode, clientbound::{
        game::{
            GameState, MenuType, ObjectPack, ServerGamePacket, ServerVoiceData, events::{
                Event, ServerEvent,
                chat::{ChatType, EventChat},
            },
        }, initial_sync::InitialSync, kick::KickClient,
    }, frame_packet, serverbound::game::actions::GameAction,
};
use glam::Vec3;
use slab::Slab;
use tokio::sync::mpsc;

use crate::{Client, ConnId, Inbound, Outbound, PlayerId, SimJoinMsg, SimMsg, player::Player, world::World};

pub mod bullets;
pub mod economy;
pub mod events;
pub mod item_logic;
pub mod item_state;
pub mod hull;
pub mod humans;
pub mod item_grid;
pub mod item_types;
pub mod items;

/// One of a client's 8 voice slots: a speaker it can hear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Earshot {
    pub player: PlayerId,
    pub human: Option<usize>,
    pub distance: f32,
    pub volume: f32,
}

/// server_events: every event ever announced, in a ring of 65536 slots that a client walks with its own count.
pub struct EventRing {
    slots: Vec<Option<Event>>,
    count: u16,
}

impl Default for EventRing {
    fn default() -> Self {
        Self { slots: (0..65536).map(|_| None).collect(), count: 0 }
    }
}

impl EventRing {
    pub fn push(&mut self, event: Event) {
        self.slots[self.count as usize] = Some(event);
        self.count = self.count.wrapping_add(1);
    }

    pub fn get(&self, idx: u16) -> Option<&Event> {
        self.slots[idx as usize].as_ref()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Event> {
        self.slots.iter().flatten()
    }
}

#[allow(unused)]
struct TickCtx<'a> {
    players: &'a Slab<Player>,
    world: &'a World,
    events: &'a EventRing,
    humans: Vec<rosa_protocol::clientbound::game::ServerHumanObject>,
    own_humans: HashMap<usize, rosa_protocol::clientbound::game::OwnHumanData>,
    heads: HashMap<usize, (Vec3, bool)>,
    items: Vec<rosa_protocol::clientbound::game::ServerItemObject>,
    tick: u32,
    gamestate: GameState,
    ready_states: Option<[bool; 32]>
}

pub struct Sim {
    tick: u32,
    round_number: u32,
    gamemode: GameMode,
    gamestate: GameState,
    in_rx: mpsc::UnboundedReceiver<Inbound>,
    out_tx: Outbound,
    world: World,
    saved_accounts: SrkData,
    players: Slab<Player>,
    clients: HashMap<ConnId, Client>,
    max_players: u8,
    events: EventRing,
    items: rosa_physics::Table<items::Item>,
    humans: rosa_physics::Table<crate::human::Human>,
    bodies: rosa_physics::RigidBodies,
    item_types: Vec<item_types::ItemType>,
    item_grid: item_grid::ItemGrid,
    noise_seed: i32,
    corporations: [economy::Corporation; economy::CORPORATIONS],
    bullets: Vec<bullets::Bullet>,
    /// The world mode clock (0xe3b21e0).
    world_time: i32,
    /// Per account, the spawn timer kept while its player is away (account record +0x48, not saved to disk).
    account_spawn_timers: HashMap<u32, i32>,
}

impl Sim {
    pub fn new(
        in_rx: mpsc::UnboundedReceiver<Inbound>,
        out_tx: Outbound,
        map_name: String,
        gamemode: GameMode,
        max_players: u8,
    ) -> Self {
        let srk_data = SrkData::load(Path::new("server.srk")).unwrap();
        let corporations = [economy::Corporation::default(); economy::CORPORATIONS];
        let mut events = EventRing::default();
        // TODO: reset_game also restocks the shop and car dealership vehicles here
        events.push(economy::stock_event(&corporations, 0));

        Self {
            tick: 0,
            in_rx,
            out_tx,
            world: World::new(map_name),
            gamemode,
            gamestate: GameState::Intermission,
            round_number: 0,
            saved_accounts: srk_data,
            players: Slab::with_capacity(256),
            clients: HashMap::new(),
            max_players,
            events,
            items: rosa_physics::Table::new(items::MAX_ITEMS),
            humans: rosa_physics::Table::new(crate::human::MAX_HUMANS),
            bodies: rosa_physics::RigidBodies::default(),
            item_types: item_types::item_types(),
            item_grid: item_grid::ItemGrid::default(),
            noise_seed: 0,
            corporations,
            bullets: Vec::new(),
            world_time: economy::WORLD_TIME_START,
            account_spawn_timers: HashMap::new(),
        }
    }

    fn apply(&mut self, input: Inbound) {
        match input.msg {
            SimMsg::Join(msg) => self.on_join(input.conn, input.src, msg),
            SimMsg::Game(g) => {
                let client = self.clients.get_mut(&input.conn).unwrap();
                let player = self.players.get_mut(client.player_id.idx()).unwrap();

                client.last_sdl_tick = g.sdl_tick;
                client.event_cursor = g.received_events as u16;
                client.pack_ack = g.unk & 0x7ff;

                player.process_game_packet(g);
            }
            SimMsg::Leave => self.on_leave(input.conn),
        }
    }

    fn send_initial_sync(&mut self, to: SocketAddr) {
        let packet = InitialSync {
            round_number: self.round_number,
            weekly_enabled: false,
            weekday: self.world.weekday as u8,
            sun_angle: self.world.sun_angle(),
            sun_axial_tilt: self.world.sun_axial_tilt(),
            versus_movedelay: None,
            gamemode: self.gamemode,
            map_name: self.world.map.map_name.clone(),
        };

        let _ = self.out_tx.send((rosa_protocol::frame_packet(packet), to));
    }

    fn send_chat(&mut self, message: &str, chat_type: ChatType, speaker_id: i32, volume: i32) {
        self.events.push(Event {
            tick_created: self.tick,
            kind: ServerEvent::Chat(EventChat {
                chat_type,
                speaker_id,
                volume,
                message: message.to_string()
            })
        });
    }

    fn kick(&mut self, src: SocketAddr, reason: &str) {
        let _ = self.out_tx.send((
            frame_packet(KickClient {
                reason: reason.to_string(),
            }),
            src,
        ));
    }

    pub fn events(&self) -> impl Iterator<Item = &Event> {
        self.events.iter()
    }

    pub fn announce_event(&mut self, event: Event) {
        self.events.push(event);
    }

    fn alloc_player(&mut self, account_id: u32, j: SimJoinMsg) -> Option<PlayerId> {
        if self.players.len() >= 255 {
            return None;
        }

        let entry = self.players.vacant_entry();
        let id = PlayerId(entry.key() as u32);

        entry.insert(Player::new_from_join(id, account_id, j));

        Some(id)
    }

    pub fn run(mut self) {
        let tick = Duration::from_millis(16);
        let mut last_tick = Instant::now();

        loop {
            std::thread::sleep(Duration::from_millis(1));
            let mut burst = 0;
            while last_tick.elapsed() > Duration::from_millis(15) {
                burst += 1;
                if burst <= 3 {
                    last_tick += tick;
                } else {
                    last_tick = Instant::now();
                }
                while let Ok(inp) = self.in_rx.try_recv() {
                    self.apply(inp);
                }
                let mut pending= HashMap::new();
                for (_, player) in self.players.iter_mut() {
                    pending.insert(player.player_id, player.actions.drain().collect::<Vec<GameAction>>());
                }

                self.process_actions(pending);
                self.start_round_if_all_ready();
                if self.gamemode == GameMode::World {
                    self.logic_world();
                }
                self.player_simulation();
                self.physics_tick();
                self.broadcast_tick();

                for player in self.saved_accounts.players.iter_mut() {
                    if player.ban_time > 0 {
                        player.ban_time -= 1;
                    }
                }

                self.tick += 1;
                if self.gamestate as u8 <= GameState::InGame as u8 {
                    self.bullet_simulation();
                    self.bullet_ttl();
                }
            }
        }
    }

    fn process_actions(&mut self, player_actions: HashMap<PlayerId, Vec<GameAction>>) {
        for (player, actions) in player_actions {
            for action in actions {
                match action {
                    GameAction::Menu(menu_action) => self.on_menu_action(player, menu_action),
                    GameAction::Chat(chat_action) => self.on_chat_action(player, chat_action),
                    GameAction::Item(item_action) => self.item_action(player, item_action.a as usize, item_action.b as i32),
                    GameAction::Inventory(inventory_action) => {
                        let human = self.players.get(player.idx()).and_then(|p| p.human);
                        if let Some(h) = human.and_then(|id| self.humans.get_mut(id)) {
                            crate::human::inventory::queue_inventory_action(h, inventory_action.a as i32, inventory_action.b as i32);
                        }
                    }
                    GameAction::Admin(admin_action) => println!("Admin {:?}", admin_action),
                    GameAction::Unknown => {},
                }
            }
        }
    }

    fn start_round_if_all_ready(&mut self) {
        if self.gamestate != GameState::Intermission || self.players.is_empty() {
            return;
        }
        if self.players.iter().all(|(_, p)| p.is_ready) {
            println!("[Sim] Everyone is ready, starting the round");

            self.gamestate = GameState::InGame;

            for (_, p) in &mut self.players {
                p.menu = MenuType::Empty;
            }
        }
    }

    fn calculate_ready_states(players: &mut Slab<Player>) -> [bool; 32] {
        let mut ready_vec = [false; 32];

        for (idx, player) in &mut *players {
            ready_vec[idx] = player.is_ready;
        }

        ready_vec
    }

    fn broadcast_tick(&mut self) {
        let humans = self.human_objects();
        let own_humans = self.own_human_data();
        let heads = self.humans.iter().map(|(id, h)| (id, (h.bones[3].pos, h.old_health > 0))).collect();
        let items = self.item_objects();
        let Sim {
            clients,
            players,
            world,
            events,
            out_tx,
            tick,
            gamestate,
            ..
        } = self;

        let ready_states = if *gamestate == GameState::Intermission {
            Some(Self::calculate_ready_states(players))
        } else {
            None
        };

        let ctx = TickCtx {
            players,
            world,
            events,
            humans,
            own_humans,
            heads,
            items,
            tick: *tick,
            gamestate: *gamestate,
            ready_states,
        };

        for client in clients.values_mut() {
            let Some(player) = players.get(client.player_id.idx()) else {
                continue;
            };

            let packet = Sim::build_game_packet(client, &ctx, player.player_id);

            let _ = out_tx.send((frame_packet(packet), client.addr));
        }
    }

    fn build_game_packet(
        client: &mut Client,
        ctx: &TickCtx,
        player_id: PlayerId
    ) -> ServerGamePacket {
        let first_event = client.event_cursor as u32;
        let (global_event_count, events) = Self::collect_events(client, ctx);
        let earshots = Self::calculate_earshot(client, player_id, ctx);
        let (object_packs, pack_offset) = Self::object_packs(client, ctx);
        let player = ctx.players.get(player_id.idx()).unwrap();

        let mut voice_data: [Option<ServerVoiceData>; 8] = [const { None }; 8];

        for (idx, earshot) in earshots.iter().enumerate() {
            let Some(earshot) = earshot else { continue };
            let speaker = ctx.players.get(earshot.player.idx()).unwrap();

            let data = ServerVoiceData {
                human_id: earshot.human.map_or(-1, |h| h as i32),
                item_id: -1,
                player_id: earshot.player.idx() as i32,
                voice_frames: speaker.voice.recent4()
            };

            voice_data[idx] = Some(data);
        }

        ServerGamePacket {
            client_id: client.player_id.0,
            received_actions: player.actions.write as u32,
            round_number: 0,
            network_tick: ctx.tick,
            last_sdl_tick: client.last_sdl_tick,
            menu_type: player.menu,
            // TODO: the store menu (10) sends something else here
            money: player.money,
            gamestate: ctx.gamestate,
            ready_states: ctx.ready_states,
            follow_pos: Vector::new(0.0, 0.0, 0.0),
            own_human: ctx.own_humans.get(&player_id.idx()).cloned(),
            humans: ctx.humans.clone(),
            items: ctx.items.clone(),
            object_packs,
            pack_offset,
            global_event_count,
            first_event,
            events,
            voice: voice_data
        }
    }

    /// object_packet_update_relevance and the pack part of append_object_packet: every object gets a slot on the
    /// client through a pack entry and gives it back with an unpack entry when it is gone; the entries go into a
    /// 2048 entry ring and every packet carries those the client has not acknowledged.
    // TODO: relevance by distance (items within 256 of the camera, humans always) and pocketed items being unpacked
    fn object_packs(client: &mut Client, ctx: &TickCtx) -> (Vec<ObjectPack>, u16) {
        const RING: u16 = 0x800;
        if client.pack_ring.is_empty() {
            client.pack_ring = vec![ObjectPack { slot: 0, unpack: true, kind: 0, item_type: 0, index: 0 }; RING as usize];
        }
        let current: HashMap<u16, ObjectPack> = ctx
            .humans
            .iter()
            .map(|h| (h.slot, ObjectPack { slot: h.slot, unpack: false, kind: 0, item_type: 0, index: h.human_id }))
            .chain(ctx.items.iter().map(|i| (i.slot, ObjectPack { slot: i.slot, unpack: false, kind: 1, item_type: i.item_type as u16, index: i.item_id })))
            .collect();
        let queue = |client: &mut Client, pack: ObjectPack| {
            let next = (client.pack_count + 1) & (RING - 1);
            if next == client.pack_ack {
                return;
            }
            client.pack_ring[client.pack_count as usize] = pack;
            client.pack_count = next;
        };
        let mut gone: Vec<u16> = client.packed.keys().filter(|s| !current.contains_key(s)).copied().collect();
        gone.sort_unstable();
        for slot in gone {
            queue(client, ObjectPack { slot, unpack: true, kind: 0, item_type: 0, index: 0 });
            client.packed.remove(&slot);
        }
        let mut changed: Vec<ObjectPack> = current.values().filter(|p| client.packed.get(&p.slot).is_none_or(|q| q.kind != p.kind || q.index != p.index)).copied().collect();
        changed.sort_unstable_by_key(|p| p.slot);
        for pack in changed {
            queue(client, pack);
            client.packed.insert(pack.slot, pack);
        }
        let pending = client.pack_count.wrapping_sub(client.pack_ack) & (RING - 1);
        let packs = (0..pending).map(|k| client.pack_ring[((client.pack_ack + k) & (RING - 1)) as usize]).collect();
        (packs, client.pack_ack)
    }

    fn collect_events(
        client: &mut Client,
        ctx: &TickCtx
    ) -> (u32, Vec<(u32, Event)>) {
        let total = ctx.events.count;
        let pending = total.wrapping_sub(client.event_cursor);
        let to_send = pending.min(0x3f); // 63 cap

        let mut out = Vec::with_capacity(to_send as usize);
        for i in 0..to_send {
            let idx = client.event_cursor.wrapping_add(i);
            let Some(ev) = ctx.events.get(idx) else {
                out.push((idx as u32, Event { tick_created: ctx.tick, kind: ServerEvent::Empty }));
                continue;
            };

            let expired = ctx.tick.wrapping_sub(ev.tick_created) > 600;
            let ephemeral = matches!(ev.kind, ServerEvent::Sound(_) | ServerEvent::PhoneSound(_));
            let kind = if expired && ephemeral {
                ServerEvent::Empty
            } else {
                ev.kind.clone()
            };

            out.push((
                idx as u32,
                Event {
                    tick_created: ev.tick_created,
                    kind,
                },
            ));
        }

        client.event_cursor = client.event_cursor.wrapping_add(to_send);
        (total as u32, out)
    }

    /// calculate_voice: keeps each speaker the client can hear in the same one of its 8 voice slots while they stay
    /// audible. Two humans hear each other within a range set by the speaker's volume level, halved without line of
    /// sight; players without humans hear each other, and everyone hears everyone while the round restarts.
    fn calculate_earshot(client: &mut Client, player_id: PlayerId, ctx: &TickCtx) -> [Option<Earshot>; 8] {
        // TODO: phones and walkie-talkies (earshots through a receiving item), the voicechat setting, and a
        // listener without a human hearing from the human it spectates
        let restarting = ctx.gamestate == GameState::Restarting;
        let listener = ctx.players.get(player_id.idx()).and_then(|p| p.human).and_then(|h| ctx.heads.get(&h).map(|&(pos, _)| pos));
        let range = |level: u8| match level {
            0 => 8.0f32,
            1 => 64.0,
            _ => 128.0,
        };
        let map = &ctx.world.map;
        let line_of_sight = |from: Vec3, to: Vec3| -> f32 {
            let d = Vec3::new(to.x - from.x, to.y - from.y, to.z - from.z);
            let dist = (d.z * d.z + (d.x * d.x + d.y * d.y)).sqrt();
            if dist <= 128.0 && crate::world::trace::line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, from, to).is_none() { 1.0 } else { 0.5 }
        };
        let distance = |a: Vec3, b: Vec3| {
            let d = Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z);
            ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt()
        };
        for slot in client.earshots.iter_mut() {
            let Some(e) = slot else { continue };
            let Some(speaker) = ctx.players.get(e.player.idx()) else {
                *slot = None;
                continue;
            };
            e.human = speaker.human;
            if speaker.voice.is_silenced {
                *slot = None;
                continue;
            }
            if restarting {
                continue;
            }
            match (listener, e.human) {
                (None, None) => {}
                (Some(pos), Some(h)) => {
                    let Some(&(head, alive)) = ctx.heads.get(&h) else {
                        *slot = None;
                        continue;
                    };
                    if !alive {
                        *slot = None;
                        continue;
                    }
                    e.distance = distance(head, pos);
                    e.volume = (line_of_sight(head, pos) - e.volume) * 0.125 + e.volume;
                    if !(e.distance <= e.volume * range(speaker.voice.volume_level)) {
                        *slot = None;
                    }
                }
                _ => *slot = None,
            }
        }
        for (_, p) in ctx.players.iter() {
            let id = p.player_id;
            if id == player_id || p.voice.is_silenced || client.earshots.iter().flatten().any(|e| e.player == id && e.human == p.human) {
                continue;
            }
            let (distance, volume) = if restarting || (listener.is_none() && p.human.is_none()) {
                (4.0, 1.0)
            } else {
                let (Some(pos), Some(&(head, alive))) = (listener, p.human.and_then(|h| ctx.heads.get(&h))) else { continue };
                if !alive {
                    continue;
                }
                let volume = line_of_sight(head, pos);
                let distance = distance(head, pos);
                if !(range(p.voice.volume_level) * volume > distance) {
                    continue;
                }
                (distance, volume)
            };
            let earshot = Earshot { player: id, human: p.human, distance, volume };
            if let Some(free) = client.earshots.iter_mut().find(|s| s.is_none()) {
                *free = Some(earshot);
                continue;
            }
            // TODO: connection_find_earshot_slot compares against the slot it last picked (starting from slot -1,
            // outside the array); this evicts the slot with the largest distance / (volume + 0.01) instead
            let ratio = |e: &Earshot| e.distance / (e.volume + 0.01);
            let worst = client.earshots.iter().enumerate().filter_map(|(i, s)| s.as_ref().map(|e| (i, ratio(e)))).max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((i, r)) = worst
                && r > distance / volume
            {
                client.earshots[i] = Some(earshot);
            }
        }
        client.earshots
    }
}
