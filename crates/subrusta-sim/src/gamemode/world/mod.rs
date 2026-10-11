use glam::Vec3;
use subrusta_physics::rotation::{IDENTITY, rotate_orientation};
use subrusta_protocol::{
    GameMode, Team,
    clientbound::game::{GameState, ItemKind, MenuType, VehicleKind},
};

use crate::time::WORLD_HOUR;
use crate::{
    PlayerId, items,
    sim::{
        Sim,
        economy::{self, CORPORATIONS},
        missions,
    },
};

pub mod corporations;
pub mod world_missions;

/// The world clock starts each day at 8 AM and moves 10 a tick; the day's missions are handed out a world second later,
/// the corporations' memos and manager pay every quarter hour until 5 PM, and the day ends past 7 PM.
const CLOCK_START: i32 = 8 * WORLD_HOUR;
const CLOCK_RATE: i32 = 10;
const MISSIONS_AT: i32 = CLOCK_START + 60;
const DAY_STEP: i32 = WORLD_HOUR / 4;
const LAST_DAY_STEP: i32 = 17 * WORLD_HOUR - 1;
const DAY_END: i32 = 19 * WORLD_HOUR - 1;
const SAVE_PERIOD: i32 = 18000;
/// A mission disk's despawn time while its mission is active.
const KEEP_DESPAWN: i32 = 65535;
const PLAY_TIME_STEP: u32 = 5;
/// Every 18000 a human's stamina ceiling drops by 2 (8 drops to 7, and 7 and below stay).
const STAMINA_FLOOR: i32 = 7;
const STAMINA_DROP: i32 = 2;
/// Every 54000 accounts lose 16 criminal rating, and a criminal player's own rating moves 8 onto their account and
/// drops by 1; a player at crimenobuy cannot buy (100 things bought, 4 vehicles).
const ACCOUNT_CRIME_DECAY: u32 = 16;
const ACCOUNT_CRIME_GAIN: u32 = 8;
const NO_BUY_ITEMS: i32 = 100;
const NO_BUY_VEHICLES: i32 = 4;
/// The world keeps 8 memos at the spawn, 4 to the west and 1 to the south of it.
const MEMOS: i32 = 8;
const MEMO_WEST: f32 = 4.0;
const MEMO_SOUTH: f32 = 1.0;
/// On test2 reset_game puts the spawn by the fifth train spawn, 16 east and 1 up.
const TEST2_SPAWN_TRAIN: usize = 4;
const TRAIN_LIFT: f32 = 2.0;
const LAB_GUARD: i32 = 0;
const TRAIN_COLOR: i32 = 1;
const TEST2_SPAWN_EAST: f32 = 16.0;
const TEST2_SPAWN_UP: f32 = 1.0;
/// load_map's spawn: one end of the first street, moved 1480 east, 1 up and 24 north, with a 16 by 16 area.
const SPAWN_END: usize = 0;
const SPAWN_EAST_A: f32 = 96.0;
const SPAWN_EAST_B: f32 = 1384.0;
const SPAWN_UP: f32 = 1.0;
const SPAWN_NORTH: f32 = 24.0;
const SPAWN_AREA: f32 = 16.0;
const SPAWN_MARGIN: f32 = 2.0;
/// enter_city: a civilian lands somewhere in the spawn area facing west; a corporation member 18 along their table
/// and 4 back from it, facing along it.
const CIVILIAN_YAW: f32 = -90.0_f32.to_radians();
const TABLE_ALONG: f32 = 18.0;
const TABLE_BACK: f32 = -4.0;
const TABLE_DROP: f32 = 0.5;
const QUARTER_TURN: f64 = 90.0_f64.to_radians();
const ENTER_CITY_MENU: u32 = 1;
const ENTER_AS_CIVILIAN: u32 = 1;
const ENTER_AS_TEAM: u32 = 2;
/// A lab gets 3 to 6 guards, each at one of its up to 64 spots.
const GUARDS_MIN: i32 = 3;
const MAX_PATROL_SPOTS: usize = 64;
const SPOT_TRIES: i32 = 512;
/// roundtime= (minutes) times 3600, 30 minutes when unset: what the world timer shows.
pub const DEFAULT_MAX_TIME: i32 = 108000;

/// config_world.txt (load_conf_world): the traffic, the crime rates and limits, the money and the join messages.
#[derive(Clone, Copy, Debug)]
pub struct WorldConfig {
    pub traffic: i32,
    pub crimecivciv: i32,
    pub crimecivteam: i32,
    pub crimeteamciv: i32,
    pub crimeteamteam: i32,
    pub crimeteamteaminbase: i32,
    pub crimevscriminal: i32,
    pub crimenobuy: i32,
    pub crimenospawn: i32,
    pub crimekick: i32,
    pub startcash: i32,
    pub mincash: i32,
    pub showjoinexit: bool,
    pub respawnteam: bool,
}

impl Default for WorldConfig {
    fn default() -> Self {
        WorldConfig {
            traffic: 256,
            crimecivciv: 100,
            crimecivteam: 200,
            crimeteamciv: 50,
            crimeteamteam: 0,
            crimeteamteaminbase: 100,
            crimevscriminal: 20,
            crimenobuy: 200,
            crimenospawn: 500,
            crimekick: 1000,
            startcash: 1000,
            mincash: 500,
            showjoinexit: true,
            respawnteam: false,
        }
    }
}

impl WorldConfig {
    /// The config in config_world.txt's `key=value` lines.
    pub(crate) fn to_text(self) -> String {
        [
            ("traffic", self.traffic),
            ("crimecivciv", self.crimecivciv),
            ("crimecivteam", self.crimecivteam),
            ("crimeteamciv", self.crimeteamciv),
            ("crimeteamteam", self.crimeteamteam),
            ("crimeteamteaminbase", self.crimeteamteaminbase),
            ("crimevscriminal", self.crimevscriminal),
            ("crimenobuy", self.crimenobuy),
            ("crimenospawn", self.crimenospawn),
            ("crimekick", self.crimekick),
            ("startcash", self.startcash),
            ("mincash", self.mincash),
            ("showjoinexit", self.showjoinexit as i32),
            ("respawnteam", self.respawnteam as i32),
        ]
        .iter()
        .map(|(k, v)| format!("{k}={v}\n"))
        .collect()
    }

    /// Reads config_world.txt, writing the defaults there first when there is no such file.
    pub(crate) fn load(path: &std::path::Path) -> Self {
        let mut c = WorldConfig::default();
        if !path.exists()
            && let Err(e) = std::fs::write(path, c.to_text())
        {
            eprintln!("[World] could not write {}: {e}", path.display());
        }
        if let Ok(text) = std::fs::read_to_string(path) {
            let num = |key: &str| text.lines().find_map(|l| l.trim().strip_prefix(key)).and_then(|v| v.trim().parse::<i32>().ok());
            c.traffic = num("traffic=").unwrap_or(c.traffic).clamp(0, 512);
            c.crimecivciv = num("crimecivciv=").unwrap_or(c.crimecivciv);
            c.crimecivteam = num("crimecivteam=").unwrap_or(c.crimecivteam);
            c.crimeteamciv = num("crimeteamciv=").unwrap_or(c.crimeteamciv);
            c.crimeteamteam = num("crimeteamteam=").unwrap_or(c.crimeteamteam);
            c.crimeteamteaminbase = num("crimeteamteaminbase=").unwrap_or(c.crimeteamteaminbase);
            c.crimevscriminal = num("crimevscriminal=").unwrap_or(c.crimevscriminal);
            c.crimenobuy = num("crimenobuy=").unwrap_or(c.crimenobuy);
            c.crimenospawn = num("crimenospawn=").unwrap_or(c.crimenospawn);
            c.crimekick = num("crimekick=").unwrap_or(c.crimekick).max(1);
            c.startcash = num("startcash=").unwrap_or(c.startcash);
            c.mincash = num("mincash=").unwrap_or(c.mincash);
            c.showjoinexit = num("showjoinexit=").map_or(c.showjoinexit, |v| v != 0);
            c.respawnteam = num("respawnteam=").map_or(c.respawnteam, |v| v != 0);
        }
        c
    }
}

/// Where a player's human stood when the day ended (player +0x3818 on), to stand again after the reset.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SavedBody {
    pub pos: Vec3,
    pub yaw: f32,
    pub vehicle: i32,
    pub seat: usize,
}

/// A vehicle a key was saved with at the end of the day (0xe3b2204 + 0x40 each): respawned at the reset where it was,
/// the key then pointing at the new one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SavedVehicle {
    pub old_id: usize,
    pub new_id: Option<usize>,
    pub kind: VehicleKind,
    pub color: i32,
    pub pos: Vec3,
    pub rot: subrusta_physics::RotMatrix,
}

/// World mode's state: its config, the day's state and the missions.
#[derive(Clone, Debug)]
pub struct WorldMode {
    pub cfg: WorldConfig,
    pub state: WorldState,
    pub missions: world_missions::WorldMissions,
}

impl WorldMode {
    pub(crate) fn load() -> Self {
        Self {
            cfg: WorldConfig::load(std::path::Path::new("config_world.txt")),
            state: WorldState { max_time: DEFAULT_MAX_TIME, ..Default::default() },
            missions: Default::default(),
        }
    }
}

/// The world mode state beyond the clock: how fast it runs, the memos placed, each corporation's memo countdown and the
/// vehicles saved for the next day.
#[derive(Clone, Debug, Default)]
pub struct WorldState {
    pub rate: i32,
    pub memos: i32,
    pub memo_timers: [i32; CORPORATIONS],
    pub saved_vehicles: Vec<SavedVehicle>,
    pub max_time: i32,
    /// What the memos at the spawn say (newspaper_load at reset_game).
    pub newspaper: Vec<u8>,
}

impl Sim {
    /// spawn_building_patrol_npcs: three to six guards (no more than the spots), each posted at a spot not yet taken
    /// (up to 512 tries), facing the building's way, with its patrol route.
    fn spawn_building_patrol_npcs(&mut self, building: usize, kind: i32) {
        let spots = self.world.map.level.buildings.get(building).map_or(Vec::new(), |b| b.patrol.clone());
        let n = spots.len();
        let a = rand::random_range(0..4);
        let b = rand::random_range(0..2);
        let base = a + b + GUARDS_MIN;
        let count = (rand::random_range(0..2) + base).min(n as i32);
        if count <= 0 {
            return;
        }
        let mut taken = [false; MAX_PATROL_SPOTS];
        for _ in 0..count {
            let mut pick = rand::random_range(0..n as i32);
            let mut tries = SPOT_TRIES;
            while taken[pick as usize] {
                pick = rand::random_range(0..n as i32);
                tries -= 1;
                if tries == 0 {
                    break;
                }
            }
            taken[pick as usize] = true;
            let spot = &spots[pick as usize];
            let yaw = match spot.facing {
                1 => 90.0_f32.to_radians(),
                2 => 180.0_f32.to_radians(),
                3 => -90.0_f32.to_radians(),
                _ => 0.0,
            };
            if let Some(id) = self.create_npc(kind, spot.pos, yaw)
                && let Some(npc) = self.npcs[id].as_mut()
            {
                npc.waypoints = spot.waypoints.clone();
            }
        }
    }
}

/// A world account is worth its money plus its shares at 95% of the price.
const SALE_SHARE: f32 = 0.95;

impl Sim {
    /// The world account bookkeeping of player_simulation: every player's account holds their money (with their shares
    /// when they belong to a corporation), their ratings and their respawn time.
    pub(crate) fn world_account_sync(&mut self) {
        for (_, p) in self.players.iter() {
            if p.account_id == u32::MAX {
                continue;
            }

            let worth = match economy::corporation_of(p.team).and_then(|c| self.corporations.get(c)) {
                Some(c) => (p.money as f32 + p.stocks as f32 * (SALE_SHARE * c.price)) as i32,
                None => p.money,
            };

            if let Some(a) = self.saved_accounts.get_player_data(p.account_id) {
                a.money = worth as u32;
                a.corp_rating = p.corp_rating as u32;
                a.crim_rating = p.crim_rating as u32;
            }

            self.account_spawn_timers.insert(p.account_id, p.spawn_timer);
        }
    }

    /// The map's spawn point: load_map's by one end of the first street, or on test2 reset_game's by the track.
    pub(crate) fn map_spawn(&self) -> Vec3 {
        if self.world.map.map_name == "test2" {
            let t = self.world.map.level.area.track.spawns.get(TEST2_SPAWN_TRAIN).map_or(Vec3::ZERO, |s| s.0);
            return Vec3::new(TEST2_SPAWN_EAST + t.x, TEST2_SPAWN_UP + t.y, t.z);
        }

        let streets = &self.world.map.streets;
        let Some(i) = streets.streets.first().and_then(|s| streets.intersections.get(s.intersections[SPAWN_END])) else { return Vec3::ZERO };
        let p = i.world_pos;

        Vec3::new((SPAWN_EAST_A + p.x) + SPAWN_EAST_B, SPAWN_UP + p.y, p.z - SPAWN_NORTH)
    }

    /// world_create_train: a train (colour 1) 2 above each of the track's train spawns, turned to its heading.
    pub(crate) fn world_create_train(&mut self) {
        let spawns = self.world.map.level.area.track.spawns.clone();
        for (i, (p, angle)) in spawns.into_iter().enumerate() {
            let mut rot = subrusta_physics::rotation::IDENTITY;

            rotate_orientation(&mut rot, Vec3::Y, angle);

            let pos = Vec3::new(p.x, p.y + TRAIN_LIFT, p.z);

            if let Some(v) = self.spawn_vehicle(VehicleKind::Train, TRAIN_COLOR, pos, rot).and_then(|id| self.vehicles.get_mut(id)) {
                v.train_index = i as i32;
            }
        }
    }

    /// logic_world.
    pub(crate) fn tick_world(&mut self) {
        self.expire_world_bots();

        if self.gamestate == GameState::Intermission {
            self.start_world();
        }

        if self.gamestate != GameState::InGame {
            return;
        }

        self.world_time += self.modes.world.state.rate;

        let clock = self.world_time;

        if clock == MISSIONS_AT {
            self.start_world_missions();
        }

        if clock % DAY_STEP == 0 && clock <= LAST_DAY_STEP {
            for k in 0..CORPORATIONS {
                self.increment_manager_rating(k);
            }

            for k in 0..CORPORATIONS {
                if self.corp_state[k].player_count > 0 {
                    if self.modes.world.state.memo_timers[k] <= 0 {
                        self.generate_team_mission_memo(k);
                    } else {
                        self.modes.world.state.memo_timers[k] -= 1;
                    }
                }
            }
        }

        if clock > DAY_END {
            return self.end_world_day();
        }

        self.stock_burger_shops();
        self.expire_corp_missions(clock);

        if clock % SAVE_PERIOD == 0 {
            self.world_save_tick();
        }

        if clock % DAY_STEP == 0 {
            self.world_crime_tick();
        }

        self.place_world_memos();

        for (_, p) in self.players.iter_mut() {
            if !p.is_bot && p.human.is_none() {
                p.menu = MenuType::EnterCity;
            }
        }

        self.keep_mission_items();
        self.world_account_sync();
    }

    /// The undecided missions whose deadline is now let go of their disks and, unless that decided them, fail.
    fn expire_corp_missions(&mut self, clock: i32) {
        for k in 0..CORPORATIONS {
            for slot in 0..missions::MISSION_SLOTS {
                let m = self.corp_state[k].missions[slot];
                if !m.active || m.done || m.deadline != clock {
                    continue;
                }
                self.despawn_missions_by_id(m.id);
                if !self.corp_state[k].missions[slot].done {
                    self.settle_mission_result(k, slot, false, 0);
                }
            }
        }
    }

    /// Each active mission's disk, while it has a despawn time, is kept from despawning.
    fn keep_mission_items(&mut self) {
        for k in 0..CORPORATIONS {
            for slot in 0..missions::MISSION_SLOTS {
                let m = self.corp_state[k].missions[slot];
                if !m.active {
                    continue;
                }
                if let Some(item) = usize::try_from(m.item).ok().and_then(|i| self.items.get_mut(i))
                    && item.despawn_time > 0
                {
                    item.despawn_time = KEEP_DESPAWN;
                }
            }
        }
    }

    /// Bots go when they lose their human, and when their deadline passes they leave with what they hold (their
    /// getaway car back to the traffic).
    fn expire_world_bots(&mut self) {
        let clock = self.world_time;
        for pid in self.players.iter().filter(|(_, p)| p.is_bot).map(|(i, _)| i).collect::<Vec<_>>() {
            let Some(p) = self.players.get(pid) else { continue };
            let (human, deadline) = (p.human, p.bot_deadline);
            if deadline == 0 || deadline >= clock {
                if human.is_none() {
                    self.players.remove(pid);
                }
                continue;
            }
            if let Some(h) = human {
                self.release_bot_human(h);
            }
            self.players.remove(pid);
        }
    }

    fn release_bot_human(&mut self, h: usize) {
        let Some(hu) = self.humans.get(h) else { return };
        if let Some(c) =
            hu.vehicle.and_then(|v| self.vehicles.get(v)).and_then(|v| usize::try_from(v.traffic_car).ok()).and_then(|t| self.traffic.cars.get_mut(t))
        {
            c.is_bot = crate::traffic::DRIVEN;
            c.is_aggressive = 0;
        }
        let held: Vec<usize> = hu.inventory.iter().flat_map(|s| s.items[..s.count.max(0) as usize].iter().map(|&i| i as usize)).collect();
        for id in held {
            if let Some(&child) = self.items.get(id).and_then(|i| i.children.first()) {
                if let Some(c) = self.items.get_mut(child) {
                    c.despawn_time = 0;
                }
                items::remove_link(&mut self.items, child, id);
            }
            if let Some(i) = self.items.get_mut(id) {
                i.despawn_time = 0;
            }
        }
        if let Some(hu) = self.humans.get_mut(h) {
            hu.despawn_ticks = 0;
            hu.player = None;
        }
    }

    /// The first tick after a reset: the day starts, with the train, the traffic and the labs' guards.
    fn start_world(&mut self) {
        self.gamestate = GameState::InGame;
        self.game_timer = self.modes.world.state.max_time;
        self.world_time = CLOCK_START;
        self.modes.world.state.rate = CLOCK_RATE;
        self.world_create_train();

        self.do_traffic = Some(self.modes.world.cfg.traffic.max(0) as u32);

        let labs: Vec<usize> =
            self.world.map.level.buildings.iter().enumerate().filter(|(_, b)| b.kind == crate::world::building::LAB).map(|(i, _)| i).collect();

        for building in labs {
            self.spawn_building_patrol_npcs(building, LAB_GUARD);
        }

        self.modes.world.state.memo_timers = [0; CORPORATIONS];
    }

    /// Every 18000: stamina ceilings drop, the dealerships and gun stores restock, and the accounts, play times and
    /// stats are saved with the share prices sent.
    fn world_save_tick(&mut self) {
        for (_, h) in self.humans.iter_mut() {
            if h.max_stamina > STAMINA_FLOOR {
                h.max_stamina = if h.max_stamina == STAMINA_FLOOR + 1 { STAMINA_FLOOR } else { h.max_stamina - STAMINA_DROP };
            }
        }
        self.restock_dealerships();
        self.stock_gun_stores();
        self.save_accounts();
        for (_, p) in self.players.iter() {
            if let Some(a) = self.saved_accounts.get_player_data(p.account_id) {
                a.play_time += PLAY_TIME_STEP;
            }
        }
        for lock in self.account_name_locks.values_mut() {
            if *lock > 0 {
                *lock -= 1;
            }
        }
        self.save_stats();
        self.events.push(economy::stock_event(&self.corporations, self.tick));
    }

    /// Every 54000: the crime pass.
    fn world_crime_tick(&mut self) {
        for a in self.saved_accounts.players.iter_mut() {
            if (a.crim_rating as i32) > 0 {
                a.crim_rating = (a.crim_rating as i32 - ACCOUNT_CRIME_DECAY as i32).max(0) as u32;
            }
        }
        for c in &mut self.corp_state {
            c.prints = 0;
        }
        let nobuy = self.modes.world.cfg.crimenobuy;
        for pid in self.players.iter().map(|(i, _)| i).collect::<Vec<_>>() {
            let Some(p) = self.players.get_mut(pid) else { continue };
            let mut crim = p.crim_rating;
            p.items_bought = 0;
            if crim > 0 {
                let account = p.account_id;
                let alive = p.human.is_some();
                if p.vehicles_bought > 0 {
                    p.vehicles_bought -= 1;
                }
                crim -= 1;
                p.crim_rating = crim;
                if alive && let Some(a) = self.saved_accounts.get_player_data(account) {
                    a.crim_rating += ACCOUNT_CRIME_GAIN;
                }
            } else if p.vehicles_bought > 0 {
                p.vehicles_bought -= 1;
            }
            let Some(p) = self.players.get_mut(pid) else { continue };
            p.bills_withdrawn = 0;
            if crim >= nobuy {
                p.items_bought = NO_BUY_ITEMS;
                p.vehicles_bought = NO_BUY_VEHICLES;
            }
        }
    }

    /// The memos at the spawn, topped back up to 8.
    fn place_world_memos(&mut self) {
        let s = self.map_spawn();
        while self.modes.world.state.memos < MEMOS {
            let pos = Vec3::new(s.x - MEMO_WEST, s.y, MEMO_SOUTH + s.z);
            let Some(id) = self.create_item(ItemKind::PaperWorld, pos, None, IDENTITY) else { break };
            let paper = self.modes.world.state.newspaper.clone();
            self.write_memo(id, &paper);
            self.modes.world.state.memos += 1;
        }
    }

    /// The end of the day: the corporations pay their players, everyone's things and whereabouts are kept, and the
    /// world starts over.
    fn end_world_day(&mut self) {
        for k in 0..CORPORATIONS {
            self.update_corp_player_ratings(k);
        }
        self.modes.world.state.saved_vehicles.clear();
        for pid in self.players.iter().map(|(i, _)| PlayerId(i as u32)).collect::<Vec<_>>() {
            self.save_inventory(pid);
            let Some(p) = self.players.get(pid.idx()) else { continue };
            let body = p.human.and_then(|h| self.humans.get(h)).map(|h| SavedBody {
                pos: h.bones[0].pos,
                yaw: h.bones[0].angles.x,
                vehicle: h.vehicle.map_or(-1, |v| v as i32),
                seat: h.seat,
            });
            if let Some(p) = self.players.get_mut(pid.idx()) {
                p.saved_body = body;
            }
        }
        self.save_accounts();
        self.reset_game();
    }

    /// The world part of reset_game: everyone back without a human (bots gone), the world emptied and the round counted.
    pub(crate) fn reset_world_players(&mut self) {
        let bots: Vec<usize> = self.players.iter().filter(|(_, p)| p.is_bot).map(|(i, _)| i).collect();
        for i in bots {
            self.players.remove(i);
        }
        for (_, p) in self.players.iter_mut() {
            p.items_bought = 0;
            p.human = None;
            p.ghost_human = false;
            p.is_ready = false;
            p.menu = MenuType::Empty;
        }
        self.clear_world();
        self.round_number += 1;
    }

    /// The world respawn of reset_game: the saved vehicles where they were, then each saved player's human where it
    /// stood, with their things and back in their seat.
    pub(crate) fn respawn_world(&mut self) {
        for k in 0..self.modes.world.state.saved_vehicles.len() {
            let sv = self.modes.world.state.saved_vehicles[k];
            self.modes.world.state.saved_vehicles[k].new_id = self.spawn_vehicle(sv.kind, sv.color, sv.pos, sv.rot);
        }
        for pid in self.players.iter().map(|(i, _)| PlayerId(i as u32)).collect::<Vec<_>>() {
            let Some(body) = self.players.get(pid.idx()).and_then(|p| p.saved_body) else { continue };
            let mut rot = IDENTITY;
            let axis = rot[1];
            rotate_orientation(&mut rot, axis, body.yaw);
            let Some(h) = self.spawn_human(body.pos, &rot, Some(pid)) else { continue };
            if let Some(hu) = self.humans.get_mut(h) {
                hu.view_yaw = body.yaw;
            }
            if let Some(p) = self.players.get_mut(pid.idx()) {
                p.human = Some(h);
            }
            self.init_player_inventory(pid);
            if body.vehicle != -1
                && let Some(v) = self.modes.world.state.saved_vehicles.iter().find(|s| s.old_id as i32 == body.vehicle).and_then(|s| s.new_id)
                && let Some(hu) = self.humans.get_mut(h)
            {
                hu.vehicle = Some(v);
                hu.seat = body.seat;
            }
        }
        for k in 0..CORPORATIONS {
            if let Some(m) = self.corp_state[k].manager.take()
                && self.players.get(m.idx()).is_some()
            {
                self.set_team_manager(k, m);
            }
        }
    }

    /// The key a world player keeps across the day: its vehicle is saved to respawn, the key remembering the old id.
    pub(crate) fn save_world_key(&mut self, v: usize) -> Option<i32> {
        let veh = self.vehicles.get(v).filter(|v| v.health > 0)?;
        self.modes.world.state.saved_vehicles.push(SavedVehicle { old_id: v, new_id: None, kind: veh.kind, color: veh.color, pos: veh.pos, rot: veh.rot });
        Some(v as i32)
    }

    /// The key's vehicle after the reset: the respawned vehicle saved under the key's old id.
    pub(crate) fn world_key_vehicle(&self, old: i32) -> Option<usize> {
        self.modes.world.state.saved_vehicles.iter().find(|s| s.old_id as i32 == old).and_then(|s| s.new_id)
    }

    /// The enter-city menu (1) of a player without a human: button 1 enters as a civilian, button 2 as they are, once
    /// their spawn wait is over and while their criminal rating is under crimenospawn.
    pub(crate) fn enter_city_menu(&mut self, pid: PlayerId, button: u32) -> bool {
        let Some(p) = self.players.get(pid.idx()) else { return false };
        if p.menu as u32 != ENTER_CITY_MENU || p.human.is_some() {
            return false;
        }
        if p.spawn_timer != 0 {
            return true;
        }
        match button {
            ENTER_AS_CIVILIAN => self.set_player_team(pid, Team::Spectator),
            ENTER_AS_TEAM => {}
            _ => return true,
        }
        if self.players.get(pid.idx()).is_some_and(|p| p.crim_rating < self.modes.world.cfg.crimenospawn) {
            self.enter_city(pid);
        }
        true
    }

    /// enter_city: a human for the player, in the spawn area as a civilian or by their corporation's table.
    pub(crate) fn enter_city(&mut self, pid: PlayerId) {
        let Some(p) = self.players.get(pid.idx()) else { return };
        let team = p.team as usize;
        let (pos, yaw) = if team >= CORPORATIONS {
            let s = self.map_spawn();
            let spread = |r: i32| ((r as f32 * (SPAWN_AREA - SPAWN_MARGIN)) * 0.5) * (1.0 / 256.0);
            let x = spread(rand::random_range(0..256)) + s.x;
            let z = spread(rand::random_range(0..256)) + s.z;
            (Vec3::new(x, s.y, z), CIVILIAN_YAW)
        } else {
            let Some(base) = self.world.map.level.bases.get(team) else { return };
            let p = Vec3::new(base.table.x, base.table.y - TABLE_DROP, base.table.z);
            let yaw = (base.table_orientation as f64 + QUARTER_TURN) as f32;
            let mut rot = IDENTITY;
            let axis = rot[1];
            rotate_orientation(&mut rot, axis, yaw);
            let (r0, r2) = (rot[0], rot[2]);
            let x = ((TABLE_BACK * r2.x) + p.x) + (r0.x * TABLE_ALONG);
            let y = (r0.y * TABLE_ALONG) + ((r2.y * TABLE_BACK) + p.y);
            let z = (TABLE_ALONG * r0.z) + (p.z + (r2.z * TABLE_BACK));
            (Vec3::new(x, y, z), yaw)
        };
        let mut rot = IDENTITY;
        let axis = rot[1];
        rotate_orientation(&mut rot, axis, yaw);
        let h = self.spawn_human(pos, &rot, Some(pid));
        let tick = self.tick;
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        p.human = h;
        if let Some(hu) = h.and_then(|h| self.humans.get_mut(h)) {
            hu.view_yaw = yaw;
        }
        let e = p.make_update_player_event(tick);
        let r = p.make_update_round_event(tick);
        p.menu = MenuType::Empty;
        self.events.push(e);
        self.events.push(r);
    }

    /// A world paper (the memos) going away lets logic_world place another.
    pub(crate) fn world_paper_deleted(&mut self) {
        self.modes.world.state.memos -= 1;
    }
}

impl Sim {
    /// The length of a world mode day in ticks (config.toml's round_time).
    pub fn set_world_max_time(&mut self, ticks: i32) {
        self.modes.world.state.max_time = ticks;
    }

    /// The world mode part of reset_game: the corporations' day reset, an intermission of no length, the players and
    /// saved vehicles respawned where they were, and the garage doors open.
    pub(crate) fn reset_world(&mut self) {
        self.reset_corporation_rounds();

        self.gamestate = GameState::Intermission;
        self.game_timer = 0;
        self.round_elapsed = 0;

        self.reset_world_players();
        self.respawn_world();

        for k in 0..Team::CORPORATIONS.len() {
            self.set_team_door(k, true);
        }

        self.apply_team_doors();
    }
}

/// The corporation base and tab menus' ids.
const CORP_MENUS_FIRST: u8 = 20;
const CORP_MENUS_LAST: u8 = 26;

/// World mode's hooks: the persistent city with its day, crime and corporations.
pub struct Hooks;

impl super::ModeHooks for Hooks {
    fn name(&self) -> &'static str {
        "world"
    }

    fn client_mode(&self) -> GameMode {
        GameMode::World
    }

    fn map(&self) -> Option<&'static str> {
        Some("test2")
    }

    fn rules(&self, _sim: &Sim) -> super::rules::ModeRules {
        super::rules::ModeRules {
            reset_traffic: None,
            crime: true,
            punish_team_kills: false,
            world_corporations: true,
            mission_spots: true,
            saved_accounts: true,
            world_start_cash: true,
            shares_follow_human: true,
            keys_keep_vehicles: true,
            respawn_wait: true,
            ready_start: false,
            ..Default::default()
        }
    }

    fn reset(&self, sim: &mut Sim) {
        sim.reset_world();
    }

    fn tick_after_traffic(&self, sim: &mut Sim) {
        sim.tick_world();
    }

    fn base_menus(&self, sim: &mut Sim, pid: PlayerId, outside: Option<Vec3>) {
        if let Some(pos) = outside {
            sim.corp_base_menu(pid, pos);
        }
        sim.corp_tab_menu(pid);
    }

    fn menus_done(&self, sim: &mut Sim) {
        sim.copy_corp_money();
    }

    fn on_menu(&self, sim: &mut Sim, pid: PlayerId, menu: u8, button: u32) -> bool {
        let open = sim.players.get(pid.idx()).is_some_and(|p| p.menu as u8 == menu && p.human.is_some());
        if !open || !(CORP_MENUS_FIRST..=CORP_MENUS_LAST).contains(&menu) {
            return false;
        }
        sim.corp_menu_action(pid, menu, button as i32);
        true
    }

    fn joining_money(&self, sim: &Sim, money: i32) -> i32 {
        money.max(sim.modes.world.cfg.mincash)
    }
}
