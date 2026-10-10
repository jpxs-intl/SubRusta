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
use tokio::sync::mpsc;

use self::traffic::traffic_section;
use crate::{Client, ConnId, Inbound, Outbound, PlayerId, SimJoinMsg, SimMsg, player::Player, world::World};

pub mod bullets;
pub mod corporations;
pub mod admin;
pub mod bots;
pub mod world;
pub mod eliminator;
pub mod missions;
pub mod round;
pub mod round_menus;
pub mod crime;
pub mod economy;
pub mod events;
pub mod item_logic;
pub mod item_state;
pub mod hull;
pub mod humans;
pub mod item_grid;
pub mod item_types;
pub mod items;
pub mod item_sets;
pub mod npcs;
pub mod shops;
pub mod tps;
pub mod traffic;
pub mod vehicles;
pub mod world_missions;
pub mod memos;
pub mod voice;

/// One of a client's 8 voice slots: a speaker it can hear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Earshot {
    pub player: PlayerId,
    pub human: Option<usize>,
    /// The phone or radio the voice comes out of, and the one it goes into (a radio's sender).
    pub item: Option<usize>,
    pub source: Option<usize>,
    pub distance: f32,
    pub volume: f32,
}

/// server_events: every event ever announced, in a ring of 65536 slots that a client walks with its own count.
pub(crate) struct EventRing {
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
    players: &'a crate::player::PlayerTable,
    world: &'a World,
    events: &'a EventRing,
    humans: Vec<rosa_protocol::clientbound::game::ServerHumanObject>,
    own_humans: HashMap<usize, rosa_protocol::clientbound::game::OwnHumanData>,
    heads: HashMap<usize, (Vec3, bool)>,
    items: Vec<rosa_protocol::clientbound::game::ServerItemObject>,
    vehicles: Vec<rosa_protocol::clientbound::game::ServerVehicleObject>,
    tick: u32,
    gamestate: GameState,
    ready_states: Option<[bool; 32]>,
    traffic: &'a crate::traffic::Traffic,
    game_timer: i32,
    round_number: u32,
    corp_round: [(i32, i32); 3],
    team_counts: [i32; 5],
    /// The items no client is sent (in a pocket or closed briefcase).
    pocketed: std::collections::HashSet<u16>,
    /// Each item with text lines: where it is, which line link slots it uses and their links.
    item_links: HashMap<u16, (Vec3, u64, [i32; crate::computer::links::ITEM_SLOTS])>,
    links: &'a crate::computer::links::LinkPool,
    voice_items: std::collections::BTreeMap<usize, voice::VoiceItem>,
    human_players: HashMap<usize, Option<PlayerId>>,
    radio_channels: &'a [Vec<usize>],
}

/// The spectated human a client sends when it watches no one.
const NO_HUMAN: u8 = 255;

/// The length of a tick.
const TICK_MS: u64 = 16;

pub struct Sim {
    pub(crate) tick: u32,
    round_number: u32,
    gamemode: GameMode,
    gamestate: GameState,
    in_rx: mpsc::UnboundedReceiver<Inbound>,
    out_tx: Outbound,
    pub(crate) world: World,
    saved_accounts: SrkData,
    players: crate::player::PlayerTable,
    pub(crate) clients: HashMap<ConnId, Client>,
    max_players: u8,
    events: EventRing,
    pub(crate) items: rosa_physics::Table<items::Item>,
    pub(crate) humans: rosa_physics::Table<crate::human::Human>,
    bodies: rosa_physics::RigidBodies,
    pub(crate) item_types: Vec<item_types::ItemType>,
    item_grid: item_grid::ItemGrid,
    noise_seed: i32,
    corporations: [economy::Corporation; economy::CORPORATIONS],
    bullets: Vec<bullets::Bullet>,
    vehicles: rosa_physics::Table<crate::vehicle::Vehicle>,
    vehicle_types: Vec<crate::vehicle::types::VehicleType>,
    /// The world mode clock (0xe3b21e0).
    world_time: i32,
    /// Per account, the spawn timer kept while its player is away (account record +0x48, not saved to disk).
    account_spawn_timers: HashMap<u32, i32>,
    /// Each account's name lock (account record +0x38, memory only): while above 0 the account's name is forced on the
    /// player at join; it counts down every world save.
    account_name_locks: HashMap<u32, i32>,
    /// The walkie-talkies on each channel (rebuilt by logic_item, read by calculate_voice).
    radio_channels: Vec<Vec<usize>>,
    traffic: crate::traffic::Traffic,
    tick_stats: tps::TickStats,
    /// Each corporation's manager, applicants and account.
    pub(crate) corp_state: [corporations::CorpState; corporations::CORPORATIONS],
    /// The vehicles the corporations can buy in round mode.
    vehicle_stock: [round_menus::VehicleOffer; round_menus::VEHICLE_STOCK],
    /// The game timer (game_mode_state +0x234), the round's elapsed ticks (+0x238) and the intermission's starting time
    /// (+0x23c).
    game_timer: i32,
    round_elapsed: i32,
    round_max_time: i32,
    round_cfg: round::RoundConfig,
    versus_cfg: round::VersusConfig,
    world_cfg: world::WorldConfig,
    world_state: world::WorldState,
    npcs: Vec<Option<npcs::Npc>>,
    admin: admin::AdminState,
    /// The round mode weekday (rounds since the weekly reset).
    weekday: i32,
    missions: missions::MissionGlobals,
    /// The percent of team damage dealt back to the attacker's head (game_mode_state +0x248), set at reset_game.
    team_damage: i32,
    /// Whether people (not bots) were on the server at the last check of the main loop.
    occupied: bool,
    stats: ServerStats,
    eliminator: eliminator::EliminatorState,
    /// The computers' volumes and file contents.
    pub(crate) fs: crate::computer::fs::FileSystem,
    pub(crate) world_missions: world_missions::WorldMissions,
    pub(crate) links: crate::computer::links::LinkPool,
}

/// What stats.txt counts since the server started: mission deals (0x7d078a4) and bullets (0x7d078a0).
#[derive(Clone, Copy, Debug, Default)]
pub struct ServerStats {
    pub missions: u32,
    pub bullets: u32,
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
        events.push(economy::stock_event(&corporations, 0));

        let mut sim = Self {
            tick: 0,
            in_rx,
            out_tx,
            world: World::new(map_name),
            gamemode,
            gamestate: GameState::Intermission,
            round_number: 0,
            saved_accounts: srk_data,
            players: crate::player::PlayerTable::default(),
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
            vehicles: rosa_physics::Table::new(crate::vehicle::MAX_VEHICLES),
            vehicle_types: crate::vehicle::types::vehicle_types(Path::new("data")),
            world_time: economy::WORLD_TIME_START,
            account_spawn_timers: HashMap::new(),
            account_name_locks: HashMap::new(),
            radio_channels: vec![Vec::new(); voice::RADIO_CHANNELS],
            traffic: crate::traffic::Traffic::default(),
            tick_stats: tps::TickStats::default(),
            corp_state: Default::default(),
            vehicle_stock: Default::default(),
            game_timer: round::ROUND_MAX_TIME,
            round_elapsed: 0,
            round_max_time: round::ROUND_MAX_TIME,
            round_cfg: round::RoundConfig::load(Path::new("config_round.txt")),
            versus_cfg: round::VersusConfig::load(Path::new("config_versus.txt")),
            world_cfg: world::WorldConfig::load(Path::new("config_world.txt")),
            world_state: world::WorldState { max_time: world::DEFAULT_MAX_TIME, ..Default::default() },
            npcs: Vec::new(),
            admin: admin::AdminState::load(Path::new("serveradmin.txt")),
            weekday: 0,
            missions: Default::default(),
            team_damage: 0,
            occupied: false,
            stats: ServerStats::default(),
            eliminator: Default::default(),
            fs: Default::default(),
            world_missions: Default::default(),
            links: Default::default(),
        };
        sim.reset_game();
        sim
    }

    fn apply(&mut self, input: Inbound) {
        if self.is_kicked(input.src.ip()) {
            return;
        }
        match input.msg {
            SimMsg::Join(msg) => self.on_join(input.conn, input.src, msg),
            SimMsg::Game(g) => {
                let Some(client) = self.clients.get_mut(&input.conn) else { return };
                client.timeout = 0;
                client.round_number = g.round_num;
                if g.round_num != self.round_number {
                    return;
                }
                let player = self.players.get_mut(client.player_id.idx()).unwrap();

                client.last_sdl_tick = g.sdl_tick;
                client.event_cursor = g.received_events as u16;
                client.pack_ack = g.unk & 0x7ff;
                client.link_ack = g.unk1;
                client.spectating = (g.spectating_human_id != NO_HUMAN).then_some(g.spectating_human_id as usize);

                player.process_game_packet(g);
            }
            SimMsg::Leave if self.clients.contains_key(&input.conn) => self.on_leave(input.conn),
            SimMsg::Leave => {}
        }
    }

    fn send_initial_sync(&mut self, to: SocketAddr) {
        let packet = self.initial_sync();
        let _ = self.out_tx.send((rosa_protocol::frame_packet(packet), to));
    }

    fn initial_sync(&self) -> InitialSync {
        InitialSync {
            round_number: self.round_number,
            weekly_enabled: self.gamemode == GameMode::Round && self.round_cfg.weekly,
            weekday: if self.gamemode == GameMode::Round { self.weekday as u8 } else { self.world.weekday as u8 },
            sun_angle: self.world.sun_angle(),
            sun_axial_tilt: self.world.sun_axial_tilt(),
            versus_movedelay: (self.gamemode == GameMode::Versus).then_some(self.versus_cfg.movedelay as u8),
            gamemode: self.gamemode.client_mode(),
            map_name: self.world.map.map_name.clone(),
        }
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

        let id = PlayerId(self.players.next_key() as u32);
        self.players.insert(Player::new_from_join(id, account_id, j));

        Some(id)
    }

    pub fn run(mut self) {
        let tick = Duration::from_millis(TICK_MS);
        let mut last_tick = Instant::now();

        loop {
            self.check_occupied();
            std::thread::sleep(Duration::from_millis(1));
            let mut burst = 0;
            while last_tick.elapsed() > Duration::from_millis(15) {
                burst += 1;
                if burst <= 3 {
                    last_tick += tick;
                } else {
                    last_tick = Instant::now();
                }
                let started = Instant::now();
                self.server_tick();
                self.tick_stats.record(started, started.elapsed());
            }
        }
    }

    /// One server tick: the inbound packets, the players' actions, the game mode, the world and the broadcast.
    pub fn server_tick(&mut self) {
        self.connection_timeouts();
        while let Ok(inp) = self.in_rx.try_recv() {
            self.apply(inp);
        }
        let mut pending= HashMap::new();
        for (_, player) in self.players.iter_mut() {
            pending.insert(player.player_id, player.actions.drain().collect::<Vec<GameAction>>());
        }

        self.process_actions(pending);
        self.admin_reset();
        if !matches!(self.gamemode, GameMode::Round | GameMode::Eliminator | GameMode::World) {
            self.start_round_if_all_ready();
        }
        self.update_shop_menus();
        self.count_corp_players();
        if self.gamemode == GameMode::Round {
            self.logic_round();
        }
        if self.gamemode == GameMode::Eliminator {
            self.logic_eliminator();
        }
        if self.gamestate == GameState::InGame {
            self.simulate_traffic();
            self.do_npc();
        }
        if self.gamemode == GameMode::World {
            self.logic_world();
        }
        self.player_simulation();
        if self.gamemode == GameMode::Round {
            self.round_account_sync();
        }
        self.physics_tick();
        self.broadcast_tick();
        self.send_admin_lists();

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
        if self.gamestate == GameState::InGame {
            self.run_bots();
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
                    GameAction::Admin(a) => self.admin_action(player, a.kind, a.a, a.b),
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

            for (_, p) in self.players.iter_mut() {
                p.menu = MenuType::Empty;
            }
        }
    }

    fn calculate_ready_states(players: &mut crate::player::PlayerTable) -> [bool; 32] {
        let mut ready_vec = [false; 32];

        for (idx, player) in players.iter_mut() {
            ready_vec[idx] = player.is_ready;
        }

        ready_vec
    }

    fn broadcast_tick(&mut self) {
        let humans = self.human_objects();
        let own_humans = self.own_human_data();
        let heads = self.humans.iter().map(|(id, h)| (id, (h.bones[3].pos, h.old_health > 0))).collect();
        let items = self.item_objects();
        let vehicles = self.vehicle_objects();
        let pocketed = self.items.iter().filter(|(_, i)| i.in_pocket).map(|(id, _)| id as u16).collect();
        let voice_items = self.voice_items();
        let human_players = voice::human_players(&self.humans);
        let item_links = self.items.iter().filter(|(_, i)| i.link_mask != 0).filter_map(|(id, i)| Some((id as u16, (self.bodies.get(i.body)?.pos, i.link_mask, i.links)))).collect();
        let corp_round = std::array::from_fn(|k| (self.corp_state[k].funds, 0));
        let team_counts = std::array::from_fn(|k| self.corp_state[k].player_count);
        let (game_timer, round_number) = (self.game_timer, self.round_number);
        let sync = self.initial_sync();
        let Sim {
            clients,
            players,
            world,
            events,
            out_tx,
            tick,
            gamestate,
            traffic,
            links,
            radio_channels,
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
            vehicles,
            tick: *tick,
            gamestate: *gamestate,
            ready_states,
            traffic,
            game_timer,
            round_number,
            corp_round,
            team_counts,
            pocketed,
            item_links,
            links,
            voice_items,
            human_players,
            radio_channels,
        };

        for client in clients.values_mut() {
            let Some(player) = players.get(client.player_id.idx()) else {
                continue;
            };

            if client.round_number != round_number {
                let _ = out_tx.send((frame_packet(sync.clone()), client.addr));
                continue;
            }
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
        let player = ctx.players.get(player_id.idx()).unwrap();
        let (global_event_count, events) = Self::collect_events(client, ctx, player.team as i32, player_id.0 as i32);
        let earshots = Self::calculate_earshot(client, player_id, ctx);
        let (object_packs, pack_offset, packed_items) = Self::object_packs(client, ctx, player.camera_pos.0);
        let (line_links, link_offset) = Self::link_section(client, ctx, player.camera_pos.0);
        let (traffic, signal) = traffic_section(client, ctx.traffic, &ctx.world.map.streets, player.camera_pos.0);

        let mut voice_data: [Option<ServerVoiceData>; 8] = [const { None }; 8];

        for (idx, earshot) in earshots.iter().enumerate() {
            let Some(earshot) = earshot else { continue };
            let speaker = ctx.players.get(earshot.player.idx()).unwrap();

            let data = ServerVoiceData {
                human_id: earshot.human.map_or(-1, |h| h as i32),
                item_id: earshot.item.map_or(-1, |i| i as i32),
                player_id: earshot.player.idx() as i32,
                voice_frames: speaker.voice.recent4()
            };

            voice_data[idx] = Some(data);
        }

        ServerGamePacket {
            client_id: client.player_id.0,
            received_actions: player.actions.write as u32,
            round_number: ctx.round_number,
            network_tick: ctx.tick,
            last_sdl_tick: client.last_sdl_tick,
            menu_type: player.menu,
            menu_tab: player.menu_tab,
            shop: if player.menu == MenuType::WorldStore {
                usize::try_from(player.menu_tab).ok().and_then(|k| ctx.world.map.level.buildings.get(k)).map_or(Vec::new(), |b| b.shop.iter().map(|e| (e.kind, e.price, e.extra)).collect())
            } else {
                Vec::new()
            },
            // TODO: the store menu (10) sends something else here
            money: player.money,
            corp_money: player.corp_money,
            corp_credit: player.corp_credit,
            corp_rating: player.corp_rating,
            crim_rating: player.crim_rating,
            manager_tab: player.manager_tab,
            menu_buttons: player.menu_buttons.clone(),
            gamestate: ctx.gamestate,
            game_timer: ctx.game_timer,
            corp_round: ctx.corp_round,
            team_counts: ctx.team_counts,
            ready_states: ctx.ready_states,
            follow_pos: Vector::new(0.0, 0.0, 0.0),
            own_human: ctx.own_humans.get(&player_id.idx()).cloned(),
            humans: ctx.humans.clone(),
            items: ctx.items.iter().filter(|i| packed_items.contains(&i.slot)).cloned().collect(),
            vehicles: ctx.vehicles.clone(),
            object_packs,
            pack_offset,
            line_links,
            link_offset,
            traffic_count: ctx.traffic.cars.len() as i32,
            traffic,
            signal,
            global_event_count,
            first_event,
            events,
            voice: voice_data
        }
    }

    /// object_packet_update_relevance and the pack part of append_object_packet: every object the client should see gets
    /// a slot through a pack entry and gives it back with an unpack entry when it is gone; the entries go into a 2048
    /// entry ring and every packet carries those the client has not acknowledged. Humans are always seen; an item is
    /// packed within 256 of the camera and unpacked past 264, or when pocketed. Returns the item slots packed.
    fn object_packs(client: &mut Client, ctx: &TickCtx, camera: Vec3) -> (Vec<ObjectPack>, u16, std::collections::HashSet<u16>) {
        const PACK_RANGE: f32 = 256.0;
        const UNPACK_RANGE: f32 = 264.0;
        const RING: u16 = 0x800;
        if client.pack_ring.is_empty() {
            client.pack_ring = vec![ObjectPack { slot: 0, unpack: true, kind: 0, item_type: 0, index: 0 }; RING as usize];
        }
        let current: HashMap<u16, ObjectPack> = ctx
            .humans
            .iter()
            .map(|h| (h.slot, ObjectPack { slot: h.slot, unpack: false, kind: 0, item_type: 0, index: h.human_id }))
            .chain(
                ctx.items
                    .iter()
                    .filter(|i| {
                        if ctx.pocketed.contains(&i.item_id) {
                            return false;
                        }
                        let d = Vec3::new(i.pos.0.x - camera.x, i.pos.0.y - camera.y, i.pos.0.z - camera.z);
                        let dist = (d.z * d.z + (d.x * d.x + d.y * d.y)).sqrt();
                        let packed = client.packed.get(&i.slot).is_some_and(|p| p.kind == 1 && p.index == i.item_id);
                        if packed { dist <= UNPACK_RANGE } else { !(PACK_RANGE <= dist) }
                    })
                    .map(|i| (i.slot, ObjectPack { slot: i.slot, unpack: false, kind: 1, item_type: i.item_type as u16, index: i.item_id })),
            )
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
        let items = client.packed.values().filter(|p| p.kind == 1).map(|p| p.slot).collect();
        (packs, client.pack_ack, items)
    }

    /// The line link part of append_object_packet: the new lines of each packed item within 4 of the camera go into
    /// the connection's 256 entry ring, and every packet carries those it has not acknowledged.
    fn link_section(client: &mut Client, ctx: &TickCtx, camera: Vec3) -> (Vec<rosa_protocol::clientbound::game::LineLinkEntry>, u8) {
        const NEAR: f32 = 4.0;
        if client.link_ring.len() != 256 {
            client.link_ring = vec![0; 256];
        }
        let mut packed: Vec<&ObjectPack> = client.packed.values().filter(|p| p.kind == 1).collect();
        packed.sort_unstable_by_key(|p| p.slot);
        let items: Vec<u16> = packed.iter().map(|p| p.index).collect();
        for item in items {
            let Some(&(pos, mask, ids)) = ctx.item_links.get(&item) else { continue };
            let d = Vec3::new(camera.x - pos.x, camera.y - pos.y, camera.z - pos.z);
            if !(NEAR > ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt()) {
                continue;
            }
            let sent = client.link_sent.entry(item as usize).or_insert(0);
            for k in 0..crate::computer::links::ITEM_SLOTS {
                if mask & (1 << k) != 0 && *sent & (1 << k) == 0 {
                    *sent |= 1 << k;
                    client.link_ring[client.link_count as usize] = ids[k];
                    client.link_count = client.link_count.wrapping_add(1);
                }
            }
        }
        let n = client.link_count.wrapping_sub(client.link_ack).min(255);
        let entries = (0..n)
            .filter_map(|k| {
                let index = client.link_ring[client.link_ack.wrapping_add(k) as usize];
                let l = ctx.links.record(index)?;
                Some(rosa_protocol::clientbound::game::LineLinkEntry { index: index as u16, kind: l.kind, tick: l.tick, item: l.item, line: l.line, text: l.text.clone(), colors: l.colors.clone() })
            })
            .collect();
        (entries, client.link_ack)
    }

    /// add_events_to_packet: the events the client has not had, up to 63, with old sounds and other teams' corporation
    /// updates sent empty.
    fn collect_events(
        client: &mut Client,
        ctx: &TickCtx,
        team: i32,
        player_id: i32
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
            let foreign = match &ev.kind {
                ServerEvent::UpdateCorporation(e) => e.corporation() != team,
                ServerEvent::Mission(e) => e.player != player_id,
                ServerEvent::Chat(c) => c.chat_type == ChatType::AdminChat && !client.admin_visible,
                _ => false,
            };
            let kind = if (expired && ephemeral) || foreign {
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

}
