use glam::Vec3;
use rosa_physics::rotation::{IDENTITY, rotate_orientation};
use rosa_protocol::{
    GameMode, Team,
    clientbound::game::{GameState, ItemKind, MenuType, events::chat::ChatType},
};

use super::{Sim, item_state::ItemState};
use crate::PlayerId;

/// The round corporations: Goldmen, Monsota and OXS.
const ROUND_CORPS: usize = 3;
/// The intermission starts at 7200 ticks; spawning begins below 5400, where the round is set up.
pub const ROUND_MAX_TIME: i32 = 7200;
const ROUND_START: i32 = 5400;
/// With nobody on a team the intermission timer rests at 6900.
const IDLE_TIME: i32 = 6900;
/// Ticks in a round day, and how many elapsed ticks pass before loot in the open starts to matter.
const DAY: i32 = 3600;
const LOOT_GRACE: i32 = 7199;
/// When the loot is all in vaults, or nobody is left alive, the round ends within a second.
const ENDGAME: i32 = 60;
const RESTART_TIME: i32 = 1260;
/// The top 10 lines go out every 120 ticks of the restart, after a header at 1250.
const TOP10_HEADER: i32 = 1250;
const TOP10_FIRST: i32 = 1200;
const TOP10_STEP: i32 = 120;
/// A player can change team once a second in the lobby, and only while more than 599 intermission ticks remain;
/// readiness can only be taken back above 180.
const TEAM_SWITCH: i32 = 60;
const LOBBY_TEAM_TIME: i32 = 599;
const LOBBY_UNREADY_TIME: i32 = 180;
/// The spawn spread: a random byte about 127, 3/128 of a unit each.
const SPAWN_SPREAD: f32 = 3.0;
const SPAWN_STEP: f32 = 1.0 / 128.0;
const SPAWN_DROP: f32 = 0.25;
/// The ties of the round corporations.
const TIES: [u8; ROUND_CORPS] = [2, 9, 8];
/// The phones on each round corporation's table: 7.75 along its back, the second 0.5 beside it, numbered from 1111.
const PHONE_BACK: f32 = 7.75;
const PHONE_SIDE: f32 = 0.5;
const PHONE_NUMBER_STEP: i32 = 1111;
const TOWN_CAR: i32 = 0;
const MINIVAN: i32 = 9;
const MINIVAN_PLAYERS: i32 = 4;
/// With weekly play on, the sixth round starts a new week.
const WEEK_DAYS: i32 = 5;
/// A new week's share market: the round corporations back to 100 shares at 100.
const WEEK_SHARES: i32 = 100;
const WEEK_PRICE: f32 = 100.0;
/// reset_game sets the clock to 11:00 outside world mode.
const CLOCK_START: i32 = 2376000;
/// The sun's roll: a quarter turn's worth of 256 steps, from a sixteenth of a turn up, and a half turn's tilt.
const SUN_STEP: f64 = 1.0 / 256.0;
const SUN_SPAN: f64 = 0.25;
const SUN_BASE: i32 = 64;
/// A share is counted at 98% of its price in a round account.
const SALE_SHARE: f32 = 0.98;
const TEAM_NAMES: [&str; ROUND_CORPS] = ["Goldmen Inc", "Monsota", "OXS International"];

/// config_round.txt (load_conf_round).
#[derive(Clone, Copy, Debug)]
pub struct RoundConfig {
    /// Round length in minutes (1 to 60).
    pub roundtime: i32,
    pub startcash: i32,
    pub weekly: bool,
    pub bonusratio: i32,
    /// Percent of team damage (0 to 200).
    pub teamdamage: i32,
}

impl Default for RoundConfig {
    fn default() -> Self {
        RoundConfig { roundtime: 10, startcash: 250, weekly: false, bonusratio: 1, teamdamage: 50 }
    }
}

impl RoundConfig {
    pub fn load(path: &std::path::Path) -> Self {
        let mut c = RoundConfig::default();
        if let Ok(text) = std::fs::read_to_string(path) {
            let num = |key: &str| text.lines().find_map(|l| l.trim().strip_prefix(key)).and_then(|v| v.trim().parse::<i32>().ok());
            c.roundtime = num("roundtime=").unwrap_or(c.roundtime).clamp(1, 60);
            c.startcash = num("startcash=").unwrap_or(c.startcash);
            c.weekly = num("weekly=").map_or(c.weekly, |v| v != 0);
            c.bonusratio = num("bonusratio=").unwrap_or(c.bonusratio);
            c.teamdamage = num("teamdamage=").unwrap_or(c.teamdamage).clamp(0, 200);
        }
        c
    }
}

/// config_versus.txt (load_conf_versus).
#[derive(Clone, Copy, Debug, Default)]
pub struct VersusConfig {
    /// Seconds at the start of a versus round in which nobody can walk (0x45385624, sent in the initial sync).
    pub movedelay: i32,
}

impl VersusConfig {
    pub fn load(path: &std::path::Path) -> Self {
        let mut c = VersusConfig::default();
        if let Ok(text) = std::fs::read_to_string(path) {
            let num = |key: &str| text.lines().find_map(|l| l.trim().strip_prefix(key)).and_then(|v| v.trim().parse::<i32>().ok());
            c.movedelay = num("movedelay=").unwrap_or(c.movedelay);
        }
        c
    }
}

/// One item kept from a player's last round (player saved_inventory, 0xc each): its type and two values (a gun's
/// loaded magazine and rounds, an item's uses, a key's vehicle type and colour, cash bills and codes).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SavedItem {
    pub kind: i32,
    pub a: i32,
    pub b: i32,
}

fn round_corp(team: Team) -> Option<usize> {
    Team::CORPORATIONS[..ROUND_CORPS].iter().position(|&t| t == team)
}

/// sprintf_comma: thousands separated by commas.
fn comma(v: i32) -> String {
    let digits = v.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if v < 0 { format!("-{out}") } else { out }
}

impl Sim {
    /// The corporation player counts of logic_simulation: active players on a corporation team with a human (in round
    /// mode, with or without).
    pub(crate) fn count_corp_players(&mut self) {
        for c in &mut self.corp_state {
            c.player_count = 0;
        }
        let round = self.gamemode == GameMode::Round;
        for (_, p) in self.players.iter() {
            if let Some(k) = Team::CORPORATIONS.iter().position(|&t| t == p.team)
                && (p.human.is_some() || round)
            {
                self.corp_state[k].player_count += 1;
            }
        }
    }

    /// logic_round: the lobby and its timer, setting up and starting the round, the round itself and the restart.
    pub(crate) fn logic_round(&mut self) {
        for (_, p) in self.players.iter_mut() {
            if p.team_switch_timer > 0 {
                p.team_switch_timer -= 1;
            }
        }
        match self.gamestate {
            GameState::Intermission => self.round_intermission(),
            GameState::InGame => self.round_in_game(),
            GameState::Restarting => self.round_restarting(),
            _ => {}
        }
    }

    fn round_intermission(&mut self) {
        let mut on_teams = 0;
        let mut all_ready = true;
        for pid in self.players.iter().map(|(_, p)| p.player_id).collect::<Vec<_>>() {
            let Some(p) = self.players.get(pid.idx()) else { continue };
            on_teams += round_corp(p.team).is_some() as i32;
            all_ready &= p.is_ready;
            if self.game_timer < ROUND_START
                && p.human.is_none()
                && let Some(k) = round_corp(p.team)
            {
                self.round_spawn(pid, k);
            }
        }
        let timer = self.game_timer;
        if on_teams == 0 {
            if self.round_max_time != timer {
                self.game_timer = timer - 1;
                if (5402..=IDLE_TIME).contains(&timer) {
                    self.game_timer = IDLE_TIME;
                    return;
                }
                return self.intermission_countdown(false);
            }
            self.close_round_doors();
            self.game_timer = timer - 1;
            if (5402..=IDLE_TIME).contains(&timer) {
                self.game_timer = IDLE_TIME;
                return;
            }
            return self.intermission_countdown(false);
        }
        if self.round_max_time == timer {
            self.close_round_doors();
        }
        self.game_timer = timer - 1;
        self.intermission_countdown(all_ready);
    }

    /// The close-door events of the first lobby tick (the doors' flags are left as they are).
    fn close_round_doors(&mut self) {
        for k in 0..ROUND_CORPS {
            self.push_team_door_event(k, false);
        }
    }

    fn intermission_countdown(&mut self, all_ready: bool) {
        if all_ready && self.game_timer > ROUND_START {
            self.game_timer = ROUND_START;
        }
        if self.game_timer == ROUND_START {
            self.round_setup();
        }
        if self.game_timer > ROUND_START - 1 {
            return;
        }
        let all_driving = self.players.iter().all(|(_, p)| {
            round_corp(p.team).is_none() || p.human.and_then(|h| self.humans.get(h)).is_none_or(|h| h.vehicle.is_some())
        });
        if all_driving {
            self.game_timer -= 7;
        }
        if self.game_timer > 0 {
            return;
        }
        for k in 0..ROUND_CORPS {
            for slot in 0..super::missions::MISSION_SLOTS {
                if self.corp_state[k].missions[slot].active {
                    self.push_mission_event(k, slot);
                }
            }
        }
        for k in 0..ROUND_CORPS {
            self.push_team_door_event(k, true);
            self.corp_state[k].door_open = true;
        }
        self.apply_team_doors();
        self.gamestate = GameState::InGame;
        self.game_timer = self.round_cfg.roundtime * DAY;
    }

    /// The round set up at 5400 ticks: traffic, the vehicle stock, the missions, the managers announced, each round
    /// corporation's town car (and minivan for more than four players) and the two phones on its table.
    fn round_setup(&mut self) {
        if !self.world.map.streets.streets.is_empty() {
            let map = &self.world.map;
            crate::traffic::spawn::create_traffic(&mut self.traffic, map, &self.vehicle_types, self.gamemode, 128);
        }
        self.randomize_corp_vehicle_stock();
        self.generate_round_missions();
        for k in 0..ROUND_CORPS {
            if let Some(m) = self.corp_state[k].manager.and_then(|m| self.players.get(m.idx())) {
                let line = format!("{} Manager: {}", TEAM_NAMES[k], m.username);
                self.send_chat(&line, ChatType::Announce, -1, 0);
            }
        }
        for k in 0..ROUND_CORPS {
            if let Some(v) = self.corporation_spawn_vehicle(TOWN_CAR, k, 0) {
                let map = &self.world.map;
                let car = crate::traffic::spawn::create_traffic_car(&mut self.traffic, map, &self.vehicle_types, 0, 0, 0, 0, 0, 0.0);
                self.traffic.cars[car].is_bot = 0;
                self.traffic.cars[car].vehicle = v as i32;
                if let Some(veh) = self.vehicles.get_mut(v) {
                    veh.traffic_car = car as i32;
                }
            }
            if self.corp_state[k].player_count > MINIVAN_PLAYERS {
                self.corporation_spawn_vehicle(MINIVAN, k, 0);
            }
            let base = &self.world.map.level.bases[k];
            let mut rot = IDENTITY;
            let axis = rot[1];
            rotate_orientation(&mut rot, axis, (base.table_orientation as f64 - (std::f64::consts::PI / 2.0)) as f32);
            let axis = rot[1];
            rotate_orientation(&mut rot, axis, std::f32::consts::PI);
            let t = base.table;
            let [r0, _, r2] = rot;
            let first = Vec3::new(PHONE_BACK * r2.x + t.x, PHONE_BACK * r2.y + t.y, PHONE_BACK * r2.z + t.z);
            let second = Vec3::new(PHONE_SIDE * r0.x + first.x, PHONE_SIDE * r0.y + first.y, PHONE_SIDE * r0.z + first.z);
            let number = (k as i32 + 1) * PHONE_NUMBER_STEP + 1;
            for (pos, texture, number) in [(first, 0, number - 1), (second, 1, number)] {
                let Some(id) = self.create_item(ItemKind::Phone, pos, None, rot) else { continue };
                if let Some(p) = self.items.get_mut(id).and_then(|i| i.state.phone_mut()) {
                    p.texture = texture;
                    p.number = number;
                }
                self.phone_update(id);
            }
        }
    }

    /// A lobby player on a round team without a human gets one at the corporation's spawn, a little spread, facing
    /// across the table, in the corporation's suit with their kept inventory.
    fn round_spawn(&mut self, pid: PlayerId, k: usize) {
        let base = &self.world.map.level.bases[k];
        let (spawn, table_orientation) = (base.spawn, base.table_orientation);
        let x = ((crate::rng::rand() & 255) as i32 - 127) as f32 * SPAWN_SPREAD * SPAWN_STEP + spawn.x;
        let z = ((crate::rng::rand() & 255) as i32 - 127) as f32 * SPAWN_SPREAD * SPAWN_STEP + spawn.z;
        let pos = Vec3::new(x, spawn.y - SPAWN_DROP, z);
        let yaw = ((std::f64::consts::PI / 2.0) + table_orientation as f64) as f32;
        let mut rot = IDENTITY;
        let axis = rot[1];
        rotate_orientation(&mut rot, axis, yaw);
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        p.customization.model = 1;
        if p.customization.suit_color <= 2 {
            p.customization.suit_color = k as u8;
        }
        p.customization.tie_color = TIES[k];
        let Some(h) = self.spawn_human(pos, &rot, Some(pid)) else { return };
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        p.human = Some(h);
        p.ghost_human = false;
        p.menu = MenuType::Empty;
        if let Some(hu) = self.humans.get_mut(h) {
            hu.view_yaw = yaw;
        }
        let e = p.make_update_player_event(self.tick);
        self.events.push(e);
        self.init_player_inventory(pid);
    }

    fn round_in_game(&mut self) {
        self.game_timer -= 1;
        self.round_elapsed += 1;
        if self.game_timer % DAY == 0 {
            for (_, p) in self.players.iter_mut() {
                p.items_bought = 0;
                if p.vehicles_bought > 0 {
                    p.vehicles_bought -= 1;
                }
                p.daily_counter = 0;
            }
        }
        if self.round_elapsed > LOOT_GRACE && !self.loot_all_in_bases() && self.all_members_home() {
            self.game_timer -= 10;
        }
        for k in 0..ROUND_CORPS {
            let team = Team::CORPORATIONS[k];
            if !self.corp_state[k].door_open && !self.players.iter().any(|(_, p)| p.team == team && p.human.is_some()) {
                self.push_team_door_event(k, true);
                self.corp_state[k].door_open = true;
                self.apply_team_doors();
            }
        }
        if self.loot_all_in_vaults() && self.game_timer > ENDGAME {
            self.game_timer = ENDGAME;
        }
        if !self.players.iter().any(|(_, p)| !p.is_bot && p.human.is_some()) && self.game_timer > ENDGAME {
            self.game_timer = ENDGAME;
        }
        if self.game_timer <= 0 {
            self.round_complete();
            self.gamestate = GameState::Restarting;
            self.game_timer = RESTART_TIME;
        }
    }

    fn is_loot(kind: ItemKind) -> bool {
        (ItemKind::DiskBlack as u8..=ItemKind::DiskRed as u8).contains(&(kind as u8)) || kind == ItemKind::CashRound
    }

    /// Whether every disk and round cash lies inside one of the round corporations' bases.
    fn loot_all_in_bases(&self) -> bool {
        let bases = &self.world.map.level.bases[..ROUND_CORPS];
        self.items.iter().filter(|(_, i)| Self::is_loot(i.item_type)).all(|(_, i)| bases.iter().any(|b| b.contains(i.pos2)))
    }

    /// Whether every round corporation member with a human is inside their own base.
    fn all_members_home(&self) -> bool {
        (0..ROUND_CORPS).all(|k| {
            let team = Team::CORPORATIONS[k];
            self.players
                .iter()
                .filter(|(_, p)| p.team == team)
                .filter_map(|(_, p)| p.human.and_then(|h| self.humans.get(h)))
                .all(|h| self.world.map.level.bases[k].contains(h.pos))
        })
    }

    /// is_position_in_vault for every disk and round cash, against any round corporation's vault.
    fn loot_all_in_vaults(&self) -> bool {
        let bases = &self.world.map.level.bases[..ROUND_CORPS];
        self.items.iter().filter(|(_, i)| Self::is_loot(i.item_type)).all(|(_, i)| bases.iter().any(|b| b.in_vault(i.pos2, 0.0)))
    }

    fn round_restarting(&mut self) {
        self.game_timer -= 1;
        let t = self.game_timer;
        if t == TOP10_HEADER {
            self.send_chat("Top 10 list", ChatType::Announce, -1, 0);
        } else if t <= TOP10_FIRST && t > 0 && (TOP10_FIRST - t) % TOP10_STEP == 0 {
            self.print_top10((TOP10_FIRST - t) / TOP10_STEP);
        }
        if t > 0 {
            return;
        }
        for (_, p) in self.players.iter_mut() {
            p.manager_tab = false;
        }
        for k in 0..ROUND_CORPS {
            let team = Team::CORPORATIONS[k];
            let price = self.corporations[k].price;
            let mut best = (0, None);
            for (i, p) in self.players.iter().filter(|(_, p)| p.team == team) {
                let worth = if self.round_cfg.weekly {
                    self.saved_accounts.players.iter().find(|a| a.account_id == p.account_id).map_or(0, |a| a.money as i32)
                } else {
                    let w = p.stocks as f32 * price + p.money as f32;
                    if !(w <= best.0 as f32) {
                        best = (w as i32, Some(i));
                    }
                    continue;
                };
                if worth > best.0 {
                    best = (worth, Some(i));
                }
            }
            if let (_, Some(i)) = best {
                if let Some(p) = self.players.get_mut(i) {
                    p.manager_tab = true;
                }
                if self.corp_state[k].manager != Some(PlayerId(i as u32)) {
                    self.corp_state[k].manager = Some(PlayerId(i as u32));
                    self.corp_state[k].fired.clear();
                }
            }
        }
        for pid in self.players.iter().map(|(_, p)| p.player_id).collect::<Vec<_>>() {
            self.save_inventory(pid);
        }
        self.reset_game();
    }

    /// print_top10_money_list: line `n` of the accounts ranked by money.
    fn print_top10(&mut self, n: i32) {
        let name = |n: &[u8]| String::from_utf8_lossy(n.split(|&b| b == 0).next().unwrap_or(&[])).into_owned();
        let mut ranked: Vec<(u32, String)> = self.saved_accounts.players.iter().map(|a| (a.money, name(&a.player_name))).collect();
        if n as usize >= ranked.len() {
            return;
        }
        ranked.sort_by_key(|&(m, _)| std::cmp::Reverse(m as i32));
        let (money, name) = &ranked[n as usize];
        let line = format!("{}. {}   ${}", n + 1, name, comma(*money as i32));
        self.send_chat(&line, ChatType::Announce, -1, 0);
    }

    /// server_main's check before each pass of its loop: when people come to an empty server the game is reset as
    /// though a week had just ended.
    pub fn check_occupied(&mut self) {
        let occupied = self.players.iter().any(|(_, p)| !p.is_bot);
        if occupied && !self.occupied {
            self.weekday = WEEK_DAYS;
            self.reset_game();
        }
        self.occupied = occupied;
    }

    /// The weekly reset of reset_game: the round corporations' share market starts over, every player has the start
    /// cash, no shares and nothing kept from last round, and the week starts at day 1.
    fn weekly_reset(&mut self) {
        for corp in &mut self.corporations[..ROUND_CORPS] {
            corp.shares = WEEK_SHARES;
            corp.price = WEEK_PRICE;
        }
        let startcash = self.round_cfg.startcash;
        for (_, p) in self.players.iter_mut() {
            p.money = startcash;
            p.stocks = 0;
            p.saved_inventory = Default::default();
        }
        self.weekday = 1;
    }

    /// The round account bookkeeping of player_simulation: outside weekly play every player's account holds their
    /// money and their shares at 98% of the price.
    pub(crate) fn round_account_sync(&mut self) {
        if self.round_cfg.weekly {
            return;
        }
        for (_, p) in self.players.iter() {
            if p.account_id == u32::MAX {
                continue;
            }
            let worth = match self.corporations.get(p.team as usize) {
                Some(c) => (p.money as f32 + p.stocks as f32 * (SALE_SHARE * c.price)) as i32,
                None => p.money,
            };
            if let Some(a) = self.saved_accounts.get_player_data(p.account_id) {
                a.money = worth as u32;
            }
        }
    }

    /// reset_game's weather outside world mode, rolled before the reseed: a sun angle (the first roll is thrown away),
    /// the clock at 11:00 and an axial tilt.
    pub(crate) fn roll_weather(&mut self) {
        let pi = f64::from_bits(0x400921fb54442eea);
        crate::rng::rand();
        let angle = ((((crate::rng::rand() & 0xff) as i32 + SUN_BASE) as f64 * pi) * SUN_SPAN * SUN_STEP) as f32;
        self.world_time = CLOCK_START;
        let tilt = (((crate::rng::rand() & 0xff) as f64 * pi) * SUN_STEP) as f32;
        self.world.set_sun(angle, tilt);
    }

    /// The round mode part of reset_game: a new round's lobby with the world emptied, the corporations' share market
    /// recounted and every player back in the lobby without a human.
    pub(crate) fn reset_round(&mut self) {
        self.save_accounts();
        self.save_stats();
        self.weekday += 1;
        if self.round_cfg.weekly && self.weekday > WEEK_DAYS {
            self.weekly_reset();
        }

        self.gamestate = GameState::Intermission;
        self.round_elapsed = 0;
        self.round_max_time = ROUND_MAX_TIME;
        self.game_timer = ROUND_MAX_TIME;
        self.reset_corporation_rounds();
        self.reset_lobby_players_and_world();
    }

    /// The corporations' part of reset_game (every dedicated mode goes through it): funds and counts cleared, the
    /// managers dropped and the share prices set to 100 on the first reset, and the shares counted again.
    // TODO: driving, racing, world, coop and versus also go through this in reset_game
    pub(crate) fn reset_corporation_rounds(&mut self) {
        let first = self.round_number == 0;
        for (k, c) in self.corp_state.iter_mut().enumerate() {
            c.funds = 0;
            c.player_count = 0;
            c.unk_31c = 0;
            c.car_space_taken.clear();
            if first {
                c.manager = None;
            }
            c.unk_328 = 0;
            let team = Team::CORPORATIONS[k];
            let corp = &mut self.corporations[k];
            if first {
                corp.price = 100.0;
            }
            let tenths = (corp.price * 10.0) as i32;
            corp.unk_10 = tenths / 10;
            corp.base_price = tenths as f32 / 10.0;
            corp.shares = 100 + self.players.iter().filter(|(_, p)| p.team == team).map(|(_, p)| p.stocks).sum::<i32>();
        }
    }

    /// The lobby part of reset_game outside world mode: every player back in the lobby without a human, the world
    /// emptied and the round counted.
    pub(crate) fn reset_lobby_players_and_world(&mut self) {
        for (_, p) in self.players.iter_mut() {
            p.items_bought = 0;
            p.vehicles_bought = 0;
            p.human = None;
            p.ghost_human = false;
            p.is_ready = false;
            p.menu = MenuType::Lobby;
        }
        self.clear_world();
        self.round_number += 1;
    }

    /// The world part of reset_game: every body, human, vehicle, item, bullet and traffic car gone, the event list
    /// started over and each connection's object and event state with it.
    fn clear_world(&mut self) {
        let gravity = self.bodies.gravity_scale;
        self.bodies = rosa_physics::RigidBodies::default();
        self.bodies.gravity_scale = gravity;
        self.humans = rosa_physics::Table::new(crate::human::MAX_HUMANS);
        self.items = rosa_physics::Table::new(super::items::MAX_ITEMS);
        self.vehicles = rosa_physics::Table::new(crate::vehicle::MAX_VEHICLES);
        self.bullets.clear();
        self.traffic = crate::traffic::Traffic::new(&self.world.map.streets, self.world.map.map_name == "round");
        self.events = super::EventRing::default();
        for c in self.clients.values_mut() {
            c.reset_for_round();
        }
    }

    /// save_inventory: what the player's human holds, kept for their next human.
    pub(crate) fn save_inventory(&mut self, pid: PlayerId) {
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        p.saved_inventory = Default::default();
        let Some(h) = p.human.and_then(|h| self.humans.get(h)) else { return };
        let mut saved: [Vec<SavedItem>; 7] = Default::default();
        for (slot, s) in h.inventory.iter().enumerate() {
            for &id in &s.items[..s.count.max(0) as usize] {
                let Some(item) = self.items.get(id as usize) else { continue };
                let kind = item.item_type as i32;
                if kind <= ItemKind::Bandage as i32 {
                    let left = item.state.left();
                    let mut entry = SavedItem { kind, a: 0, b: left };
                    if let Some(mag) = item.children.first().and_then(|&c| self.items.get(c)) {
                        let rounds = mag.state.left();
                        entry.a = 1;
                        entry.b = rounds;
                        if left > 0 && rounds <= self.item_types[mag.item_type as usize].magazine_ammo {
                            entry.b = rounds + 1;
                        }
                    }
                    saved[slot].push(entry);
                } else if item.item_type == ItemKind::Key {
                    if let ItemState::Key { vehicle: Some(v) } = item.state
                        && let Some(veh) = self.vehicles.get(v).filter(|v| v.health > 0)
                    {
                        // TODO: world mode keeps the vehicle itself (its id and pose) to respawn at reset
                        saved[slot].push(SavedItem { kind, a: veh.kind as i32, b: veh.color });
                    }
                } else if let ItemState::Cash(c) = &item.state {
                    saved[slot].push(SavedItem { kind, a: c.bills, b: c.codes as i32 });
                }
            }
        }
        if let Some(p) = self.players.get_mut(pid.idx()) {
            p.saved_inventory = saved;
        }
    }

    /// init_player_inventory: the player's kept items back on their new human (a gun with its magazine in, a key with
    /// its vehicle respawned in the base).
    pub(crate) fn init_player_inventory(&mut self, pid: PlayerId) {
        let Some(p) = self.players.get(pid.idx()) else { return };
        let Some(human) = p.human else { return };
        let team = p.team;
        let saved = p.saved_inventory.clone();
        let Some(pos) = self.humans.get(human).map(|h| h.bones[0].pos) else { return };
        for (slot, items) in saved.iter().enumerate() {
            for s in items {
                let Ok(kind) = ItemKind::try_from(s.kind as u8) else { continue };
                let item = if s.kind <= ItemKind::Bandage as i32 {
                    let Some(id) = self.create_item(kind, pos, None, IDENTITY) else { continue };
                    if !self.item_types[s.kind as usize].is_gun {
                        if let Some(i) = self.items.get_mut(id) {
                            i.state.set_left(s.b);
                        }
                    } else if s.a > 0
                        && let Ok(mag_kind) = ItemKind::try_from(s.kind as u8 + 1)
                        && let Some(mag) = self.create_item(mag_kind, pos, None, IDENTITY)
                    {
                        if let Some(i) = self.items.get_mut(mag) {
                            i.state.set_left(s.b);
                        }
                        super::items::attach_child(&mut self.items, &self.item_types, id, mag);
                    }
                    id
                } else if kind == ItemKind::Key {
                    let Some(k) = round_corp(team) else { continue };
                    let Some(v) = self.corporation_spawn_vehicle(s.a, k, s.b) else { continue };
                    let Some(id) = self.create_item(ItemKind::Key, pos, None, IDENTITY) else { continue };
                    if let Some(i) = self.items.get_mut(id) {
                        i.state = ItemState::Key { vehicle: Some(v) };
                    }
                    id
                } else if kind == ItemKind::CashWorld {
                    let Some(id) = self.create_item(kind, pos, None, IDENTITY) else { continue };
                    if let Some(c) = self.items.get_mut(id).and_then(|i| i.state.cash_mut()) {
                        c.bills = s.a;
                        c.codes = s.b as u32;
                    }
                    id
                } else {
                    continue;
                };
                self.link_to_slot(human, item, slot);
            }
        }
    }

    pub(crate) fn link_to_slot(&mut self, human: usize, item: usize, slot: usize) {
        let Sim { humans, bodies, item_grid, items, item_types, vehicles, vehicle_types, .. } = self;
        let Some(h) = humans.get_mut(human) else { return };
        let mut touch = super::items::Touchables { grid: item_grid, items, types: item_types, vehicles, vehicle_types, occupied: Vec::new() };
        crate::human::inventory::link_item_to_human(h, human, bodies, &mut touch, item, slot);
    }

    /// The lobby buttons (menu 2) in a round's intermission: 1 to 3 join a corporation (not one that fired the
    /// account), 4 leaves to spectate (a manager stops managing), 5 readies up or, early enough, back down.
    pub(crate) fn round_lobby(&mut self, pid: PlayerId, button: u32) {
        if self.gamestate != GameState::Intermission {
            return;
        }
        let versus = self.gamemode == GameMode::Versus;
        let timer = self.game_timer;
        let Some(p) = self.players.get(pid.idx()) else { return };
        if (timer > LOBBY_TEAM_TIME || versus) && p.team_switch_timer == 0 {
            match button {
                1..=3 => {
                    let k = button as usize - 1;
                    if self.corp_state[k].fired.contains(&p.account_id) {
                        return;
                    }
                    let p = self.players.get_mut(pid.idx()).unwrap();
                    p.team = Team::CORPORATIONS[k];
                    p.team_switch_timer = TEAM_SWITCH;
                    p.is_ready = false;
                }
                4 => {
                    if let Some(k) = Team::CORPORATIONS.iter().position(|&t| t == p.team)
                        && self.corp_state[k].manager == Some(pid)
                    {
                        self.corp_state[k].manager = None;
                        self.corp_state[k].fired.clear();
                    }
                    let mode = self.gamemode;
                    let p = self.players.get_mut(pid.idx()).unwrap();
                    let held = p.stocks;
                    super::economy::sell_stocks(p, &mut self.corporations, held, mode);
                    p.team = Team::Spectator;
                    p.manager_tab = false;
                    let e = p.make_update_round_event(self.tick);
                    self.events.push(e);
                    let p = self.players.get_mut(pid.idx()).unwrap();
                    p.team_switch_timer = TEAM_SWITCH;
                    p.is_ready = false;
                }
                _ => {}
            }
        }
        if button == 5 {
            let p = self.players.get_mut(pid.idx()).unwrap();
            if !p.is_ready {
                p.is_ready = true;
            } else if timer > LOBBY_UNREADY_TIME {
                p.is_ready = false;
            }
        }
        if let Some(p) = self.players.get(pid.idx()) {
            let e = p.make_update_player_event(self.tick);
            self.events.push(e);
        }
    }
}

/// Hooks for checking the round state machine against the original server.
impl Sim {
    /// One logic_round call, without the player counts logic_simulation makes before it.
    pub fn round_tick(&mut self) {
        self.logic_round();
    }

    pub fn set_game_timer(&mut self, t: i32) {
        self.game_timer = t;
    }

    /// The game state, the game timer and the round's elapsed ticks.
    pub fn round_status(&self) -> (GameState, i32, i32) {
        (self.gamestate, self.game_timer, self.round_elapsed)
    }

    pub fn vehicle_positions(&self) -> Vec<Vec3> {
        self.vehicles.iter().filter_map(|(_, v)| self.bodies.get(v.body)).map(|b| b.pos).collect()
    }

    pub fn traffic_count(&self) -> usize {
        self.traffic.cars.len()
    }
}

/// Hooks for checking the weekly reset and account bookkeeping against the original server.
impl Sim {
    /// A player of `team` with `money`, `stocks`, two items kept in its first slot and an account `account`.
    pub fn make_account_player(&mut self, team: Team, money: i32, stocks: i32, account: u32) -> PlayerId {
        let pid = self.create_player().unwrap();
        self.saved_accounts.get_or_create(account, "", 0, 0);
        if let Some(a) = self.saved_accounts.get_player_data(account) {
            a.money = 0;
        }
        let p = self.players.get_mut(pid.idx()).unwrap();
        p.is_bot = false;
        p.team = team;
        p.money = money;
        p.stocks = stocks;
        p.account_id = account;
        p.saved_inventory[0] = vec![SavedItem::default(); 2];
        pid
    }

    pub fn account_money(&mut self, account: u32) -> u32 {
        self.saved_accounts.get_player_data(account).map_or(0, |a| a.money)
    }

    pub fn set_weekly(&mut self, weekly: bool) {
        self.round_cfg.weekly = weekly;
    }

    pub fn set_startcash(&mut self, cash: i32) {
        self.round_cfg.startcash = cash;
    }

    pub fn weekday(&self) -> i32 {
        self.weekday
    }

    pub fn set_weekday(&mut self, d: i32) {
        self.weekday = d;
    }

    pub fn share_count(&self, k: usize) -> i32 {
        self.corporations[k].shares
    }

    pub fn run_account_sync(&mut self) {
        self.round_account_sync();
    }

    pub fn run_reset_game(&mut self) {
        self.reset_game();
    }
}
