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
            GameState, MenuType, ServerGamePacket, ServerVoiceData, events::{
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

pub mod events;
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

#[allow(unused)]
struct TickCtx<'a> {
    players: &'a Slab<Player>,
    world: &'a World,
    events: &'a [Event],
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
    // TODO: Leaks. Just like restart your server or whatever, but this should be a ring.
    events: Vec<Event>,
    items: rosa_physics::Table<items::Item>,
    humans: rosa_physics::Table<crate::human::Human>,
    bodies: rosa_physics::RigidBodies,
    item_types: Vec<item_types::ItemType>,
    item_grid: item_grid::ItemGrid,
    noise_seed: i32,
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
            events: Vec::new(),
            items: rosa_physics::Table::new(items::MAX_ITEMS),
            humans: rosa_physics::Table::new(crate::human::MAX_HUMANS),
            bodies: rosa_physics::RigidBodies::default(),
            item_types: item_types::item_types(),
            item_grid: item_grid::ItemGrid::default(),
            noise_seed: 0,
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

    pub fn events(&self) -> &[Event] {
        &self.events
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
                self.player_simulation();
                self.physics_tick();
                self.broadcast_tick();

                self.tick += 1;
            }
        }
    }

    fn process_actions(&mut self, player_actions: HashMap<PlayerId, Vec<GameAction>>) {
        for (player, actions) in player_actions {
            for action in actions {
                match action {
                    GameAction::Menu(menu_action) => self.on_menu_action(player, menu_action),
                    GameAction::Chat(chat_action) => self.on_chat_action(player, chat_action),
                    GameAction::Item(item_action) => println!("Item {:?}", item_action),
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
            events: events.as_slice(),
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
            money: 1000,
            gamestate: ctx.gamestate,
            ready_states: ctx.ready_states,
            follow_pos: Vector::new(0.0, 0.0, 0.0),
            own_human: ctx.own_humans.get(&player_id.idx()).cloned(),
            humans: ctx.humans.clone(),
            items: ctx.items.clone(),
            global_event_count,
            first_event,
            events,
            voice: voice_data
        }
    }

    fn collect_events(
        client: &mut Client,
        ctx: &TickCtx
    ) -> (u32, Vec<(u32, Event)>) {
        let total = ctx.events.len() as u16;
        let pending = total.wrapping_sub(client.event_cursor);
        let to_send = pending.min(0x3f); // 63 cap

        let mut out = Vec::with_capacity(to_send as usize);
        for i in 0..to_send {
            let idx = client.event_cursor.wrapping_add(i);
            // TODO: the real server keeps events in a 65536-slot ring and sends whatever is in a slot; a client whose count
            // is ahead of ours (left over from an earlier session) gets empty events until its count wraps round
            let Some(ev) = ctx.events.get(idx as usize) else {
                out.push((idx as u32, Event { tick_created: ctx.tick, kind: ServerEvent::Empty }));
                continue;
            };

            let expired = ctx.tick.wrapping_sub(ev.tick_created) > 600;
            let ephemeral = matches!(ev.kind, ServerEvent::Sound(_));
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
