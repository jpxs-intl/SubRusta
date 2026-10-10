use glam::Vec3;
use rosa_physics::rotation::{IDENTITY, rotate_orientation};
use rosa_protocol::{
    Team,
    clientbound::game::{
        GameState, ItemKind, MenuType, VehicleKind,
        events::{Event, ServerEvent, mission::EventMission},
    },
};

use super::{Sim, missions::MISSION_SPOTS};
use crate::PlayerId;

/// Up to eight groups of three: a target, the eliminator after them and the target's protector.
pub const MAX_GROUPS: usize = 8;
/// The lobby waits at 1800 ticks with two or fewer ready, and drops to 180 once everyone is ready.
const LOBBY_TIME: i32 = 1800;
const ALL_READY_TIME: i32 = 180;
const GAME_TIME: i32 = 54000;
const RESTART_TIME: i32 = 1200;
const TRAFFIC_CARS: i32 = 128;
/// The streets' intersections 18, 40, 50 and 51 are never spawned at (a bit mask over the first 52).
const BLOCKED_INTERSECTIONS: u64 = 0xc000100040000;
const LAST_MASKED_INTERSECTION: usize = 0x33;
const SPAWN_LIFT: f32 = 1.0;
const LANE_WIDTH: f32 = 4.0;
/// Positions start going out after 7200 ticks, to the eliminators from 10800; supplies drop every 3600 after that.
const REVEAL_START: i32 = 7199;
const ELIMINATOR_REVEAL_START: i32 = 0x2a2f;
const DROP_PERIOD: u32 = 3600;
/// The tables' items are 0.375 apart; a restock first clears 16 such steps within 0.5.
const TABLE_STEP: f32 = 0.375;
const CLEAR_STEPS: usize = 16;
const CLEAR_RADIUS: f32 = 0.5;
const CLEAR_DROP: f32 = 0.5;
const TABLE_GUNS: usize = 3;
const DROP_SPREAD: f32 = 0.25;
const DROP_GUN_SPREAD: f32 = 1.0;
/// The winners' scores in the corporations' funds: the targets' side or the eliminators'.
const SCORE: i32 = 100;
/// The guns a supply drop or restock picks from, with their magazines.
const GUNS: [(ItemKind, ItemKind); 5] = [
    (ItemKind::Pistol, ItemKind::PistolMag),
    (ItemKind::Mp5, ItemKind::Mp5Mag),
    (ItemKind::Ak47, ItemKind::Ak47Mag),
    (ItemKind::M16, ItemKind::M16Mag),
    (ItemKind::Uzi, ItemKind::UziMag),
];
/// The car on each corporation's table: a random pick of three types.
const TABLE_CARS: [VehicleKind; 3] = [VehicleKind::Hatchback, VehicleKind::Beamer, VehicleKind::Test];
const PHONE_NUMBER_STEP: i32 = 1111;

/// One eliminator group (0x24 bytes): the target (+0), the eliminator (+4) and the protector (+8), where the target
/// was last revealed to be (+0xc) and where they are now (+0x18).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EliminatorGroup {
    pub target: i32,
    pub eliminator: i32,
    pub protector: i32,
    pub revealed: Vec3,
    pub current: Vec3,
}

/// The eliminator globals (0x45385340 on): the groups (stale ones are kept past `count`), the last known target
/// position (0x453855c0) and the ready players counted at the start (0x4538533c).
#[derive(Clone, Copy, Debug, Default)]
pub struct EliminatorState {
    pub count: usize,
    pub groups: [EliminatorGroup; MAX_GROUPS],
    pub last_target: Vec3,
    pub ready: i32,
}

fn rand() -> i32 {
    crate::rng::rand() as i32
}

fn step(p: Vec3, r: Vec3) -> Vec3 {
    Vec3::new(r.x * TABLE_STEP + p.x, r.y * TABLE_STEP + p.y, r.z * TABLE_STEP + p.z)
}

/// A random gun and its magazine.
fn random_gun() -> (ItemKind, ItemKind) {
    GUNS[(rand() % 5) as usize]
}

impl Sim {
    /// logic_eliminator: the lobby, the hunt and the restart.
    pub(crate) fn logic_eliminator(&mut self) {
        match self.gamestate {
            GameState::Intermission => self.eliminator_lobby(),
            GameState::InGame => self.eliminator_game(),
            GameState::Restarting => {
                self.game_timer -= 1;
                if self.game_timer <= 0 {
                    self.reset_game();
                }
            }
            _ => {}
        }
    }

    /// The eliminator part of reset_game: the lobby at 1800 ticks and no groups.
    pub(crate) fn reset_eliminator(&mut self) {
        self.gamestate = GameState::Intermission;
        self.game_timer = LOBBY_TIME;
        self.round_elapsed = 0;
        self.eliminator.count = 0;
        self.reset_corporation_rounds();
        self.reset_lobby_players_and_world();
    }

    fn eliminator_lobby(&mut self) {
        let people: Vec<bool> = self.players.iter().filter(|(_, p)| !p.is_bot).map(|(_, p)| p.is_ready).collect();
        let ready = people.iter().filter(|&&r| r).count();
        let all = people.iter().all(|&r| r);
        let t = self.game_timer - 1;
        if ready <= 2 {
            self.game_timer = t.max(LOBBY_TIME);
            return;
        }
        if t > ALL_READY_TIME && all {
            self.game_timer = ALL_READY_TIME;
            return;
        }
        self.game_timer = t;
        if t <= 0 {
            self.eliminator_setup();
        }
    }

    /// The hunt begins: traffic, the groups dealt out of the ready players, everyone ready spawned at a street
    /// corner of their own and told their role, the corporations' tables stocked and their doors opened.
    fn eliminator_setup(&mut self) {
        if !self.world.map.streets.streets.is_empty() {
            let map = &self.world.map;
            crate::traffic::spawn::create_traffic(&mut self.traffic, map, &self.vehicle_types, self.gamemode, TRAFFIC_CARS);
        }
        let e = &mut self.eliminator;
        e.count = 0;
        e.groups[0].target = 0;
        e.groups[0].eliminator = 0;
        e.groups[0].protector = 0;
        let mut used = vec![false; self.world.map.streets.intersections.len()];
        let ready: Vec<i32> = self.players.iter().filter(|(_, p)| p.is_ready && !p.is_bot).map(|(i, _)| i as i32).collect();
        self.eliminator.ready = ready.len() as i32;
        self.deal_groups(ready);
        let spawning: Vec<PlayerId> = self.players.iter().filter(|(_, p)| p.is_ready && !p.is_bot).map(|(i, _)| PlayerId(i as u32)).collect();
        for pid in spawning {
            self.eliminator_spawn(pid, &mut used);
        }
        self.stock_eliminator_tables();
        for k in 0..3 {
            self.push_team_door_event(k, true);
            self.corp_state[k].door_open = true;
        }
        self.apply_team_doors();
        for i in 0..self.eliminator.count {
            let target = self.eliminator.groups[i].target;
            let pos = self.player_human_pos(target).unwrap_or(Vec3::ZERO);
            self.eliminator.groups[i].current = pos;
        }
        self.gamestate = GameState::InGame;
        self.game_timer = GAME_TIME;
        self.round_elapsed = 0;
    }

    /// The groups: up to eight, three players each drawn at random (eliminator, target, protector), going on while at
    /// least 3 to 5 players are left; the first group is made even with nobody ready.
    fn deal_groups(&mut self, mut list: Vec<i32>) {
        let least = (rand() % 3 + 3) as usize;
        let mut first = true;
        loop {
            if !first && (self.eliminator.count > MAX_GROUPS - 1 || list.len() < least) {
                break;
            }
            first = false;
            let n = self.eliminator.count;
            let g = &mut self.eliminator.groups[n];
            if !list.is_empty() {
                let i = (rand() % list.len() as i32) as usize;
                g.eliminator = list.remove(i);
                if !list.is_empty() {
                    let i = (rand() % list.len() as i32) as usize;
                    g.target = list.remove(i);
                    if !list.is_empty() {
                        let i = (rand() % list.len() as i32) as usize;
                        g.protector = list.remove(i);
                    }
                }
            }
            self.eliminator.count += 1;
        }
    }

    /// A ready player at a street corner nobody else took (the corner's position, one up, moved along its lanes), as
    /// a plain suit on team 0, immortal as an eliminator, then told their role.
    fn eliminator_spawn(&mut self, pid: PlayerId, used: &mut [bool]) {
        let map = &self.world.map;
        let inters = &map.streets.intersections;
        let n = inters.len() as i32;
        let r = if n > 0 {
            let mut r = rand() % n;
            loop {
                let ru = r as usize;
                let blocked = ru <= LAST_MASKED_INTERSECTION && BLOCKED_INTERSECTIONS >> ru & 1 != 0;
                if !blocked && !used[ru] {
                    break;
                }
                r = rand() % n;
            }
            used[r as usize] = true;
            r as usize
        } else {
            0
        };
        let Some(inter) = inters.get(r) else { return };
        let mut pos = Vec3::new(inter.world_pos.x, inter.world_pos.y + SPAWN_LIFT, inter.world_pos.z);
        let mut rot = IDENTITY;
        let axis = rot[1];
        rotate_orientation(&mut rot, axis, 0.0);
        pos.x += (inter.lanes[2] - 1) as f32 * LANE_WIDTH;
        pos.z += LANE_WIDTH * (inter.lanes[3] - 1) as f32;
        let p = self.players.get_mut(pid.idx()).unwrap();
        p.team = Team::Goldmen;
        p.customization.model = 1;
        p.customization.suit_color = 0;
        let h = self.spawn_human(pos, &rot, Some(pid));
        let p = self.players.get_mut(pid.idx()).unwrap();
        p.human = h;
        if let Some(h) = h {
            let eliminator = self.eliminator.groups[..self.eliminator.count].iter().any(|g| g.eliminator == pid.0 as i32);
            let hu = self.humans.get_mut(h).unwrap();
            if eliminator {
                hu.old_health = 100;
                hu.is_immortal = true;
            }
            hu.view_yaw = 0.0;
        }
        let p = self.players.get_mut(pid.idx()).unwrap();
        p.menu = MenuType::Empty;
        let e = p.make_update_player_event(self.tick);
        self.events.push(e);
        let me = pid.0 as i32;
        let mut told = false;
        for i in 0..self.eliminator.count {
            let g = self.eliminator.groups[i];
            if g.eliminator == me {
                told = true;
                self.push_mission(me, 0, 1, 0, None);
            } else if g.protector == me {
                told = true;
                self.push_mission(me, 0, 2, (me << 10) + g.target, None);
            }
        }
        if !told {
            self.push_mission(me, 0, 0, 0, None);
        }
    }

    /// The three corporations' tables: a car in the base, then along the table three pistols with three magazines
    /// each, a bandage or two, a phone and a soccer ball.
    fn stock_eliminator_tables(&mut self) {
        for k in 0..3 {
            let kind = match rand() & 15 {
                0..=5 => TABLE_CARS[0],
                6..=10 => TABLE_CARS[1],
                _ => TABLE_CARS[2],
            };
            let color = rand() % 6;
            let vid = self.corporation_spawn_vehicle(kind, k, color);
            let map = &self.world.map;
            let car = crate::traffic::spawn::create_traffic_car(&mut self.traffic, map, &self.vehicle_types, kind, 0, 0, 0, 0, 0.0);
            self.traffic.cars[car].is_bot = 0;
            self.traffic.cars[car].vehicle = vid.map_or(-1, |v| v as i32);
            if let Some(v) = vid.and_then(|v| self.vehicles.get_mut(v)) {
                v.traffic_car = car as i32;
            }
            let base = &self.world.map.level.bases[k];
            let rot = base.frame();
            let r0 = rot[0];
            let mut p = base.table;
            for _ in 0..TABLE_GUNS {
                rand();
                self.create_item(ItemKind::Pistol, p, None, rot);
                for _ in 0..3 {
                    p = step(p, r0);
                    self.create_item(ItemKind::PistolMag, p, None, rot);
                }
                p = step(p, r0);
            }
            let two = rand() & 1 != 0;
            self.create_item(ItemKind::Bandage, p, None, rot);
            p = step(p, r0);
            if two {
                self.create_item(ItemKind::Bandage, p, None, rot);
                p = step(p, r0);
            }
            let phone = self.create_item(ItemKind::Phone, p, None, rot);
            p = step(p, r0);
            if let Some(id) = phone {
                if let Some(ph) = self.items.get_mut(id).and_then(|i| i.state.phone_mut()) {
                    ph.texture = 0;
                    ph.number = (k as i32 + 1) * PHONE_NUMBER_STEP;
                }
                self.phone_update(id);
            }
            self.create_item(ItemKind::SoccerBall, p, None, rot);
        }
    }

    fn player_human_pos(&self, player: i32) -> Option<Vec3> {
        let h = usize::try_from(player).ok().and_then(|p| self.players.get(p)).and_then(|p| p.human)?;
        self.humans.get(h).map(|h| h.bones[0].pos)
    }

    fn player_alive(&self, player: i32) -> bool {
        usize::try_from(player).ok().and_then(|p| self.players.get(p)).is_some_and(|p| p.human.is_some())
    }

    fn push_mission(&mut self, player: i32, kind: i32, role: i32, value: i32, pos: Option<Vec3>) {
        let pos = pos.map_or([0.0; 3], |p| p.to_array());
        let e = EventMission { player, kind, role, value, pos };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::Mission(e) });
    }

    /// The hunt: the targets' positions go out at shrinking intervals after two minutes (to the protectors, and from
    /// three minutes, where they were last time to the eliminators), supplies drop every hour of clock from three
    /// minutes with some tables restocked, and the game ends when the targets or the eliminators are all gone, or the
    /// time runs out with targets left.
    fn eliminator_game(&mut self) {
        self.game_timer -= 1;
        self.round_elapsed += 1;
        let ready = self.players.iter().filter(|(_, p)| !p.is_bot && p.is_ready).count();
        let elapsed = self.round_elapsed;
        let interval = if elapsed <= 0x464f {
            0xa8c
        } else if elapsed <= 0x707f {
            0x708
        } else if elapsed <= 0x9aaf {
            0x384
        } else {
            0x12c
        };
        let n = self.eliminator.count;
        if let Some(pos) = (0..n).filter_map(|i| self.player_human_pos(self.eliminator.groups[i].target)).last() {
            self.eliminator.last_target = pos;
        }
        enum Next {
            Drop,
            Check,
            CheckEliminators,
        }
        let mut next = Next::Check;
        if elapsed > REVEAL_START {
            let mut checked_drop = false;
            if self.game_timer % interval == 0 {
                if n == 0 {
                    next = if elapsed > ELIMINATOR_REVEAL_START { Next::Drop } else { Next::CheckEliminators };
                    checked_drop = true;
                } else {
                    for i in 0..n {
                        let g = &mut self.eliminator.groups[i];
                        g.revealed = g.current;
                        let g = *g;
                        if self.round_elapsed > ELIMINATOR_REVEAL_START {
                            self.push_mission(g.eliminator, 1, 0, 0, Some(g.revealed));
                        }
                        let now = self.player_human_pos(g.target).unwrap_or(self.eliminator.last_target);
                        self.eliminator.groups[i].current = now;
                        self.push_mission(g.protector, 1, 0, 0, Some(now));
                    }
                }
            }
            if !checked_drop && self.round_elapsed > ELIMINATOR_REVEAL_START && (self.round_elapsed as u32).is_multiple_of(DROP_PERIOD) {
                next = Next::Drop;
            }
        }
        if let Next::Drop = next {
            self.eliminator_drop(ready);
            next = Next::Check;
        }
        let groups = self.eliminator.groups;
        let n = self.eliminator.count;
        let eliminators = matches!(next, Next::Check) && (0..n).any(|i| self.player_alive(groups[i].eliminator));
        if !eliminators {
            self.set_eliminator_scores(SCORE, 0, SCORE);
            if n == 0 {
                return self.end_eliminator(0, SCORE, 0);
            }
        }
        if !(0..n).any(|i| self.player_alive(groups[i].target)) {
            return self.end_eliminator(0, SCORE, 0);
        }
        if !eliminators {
            return self.end_eliminator(SCORE, 0, SCORE);
        }
        if self.game_timer > 0 {
            return;
        }
        self.end_eliminator(SCORE, 0, SCORE);
    }

    fn set_eliminator_scores(&mut self, a: i32, b: i32, c: i32) {
        self.corp_state[0].funds = a;
        self.corp_state[1].funds = b;
        self.corp_state[2].funds = c;
    }

    /// The end: the scores in the corporations' funds, the restart countdown and every group's result to everyone.
    fn end_eliminator(&mut self, a: i32, b: i32, c: i32) {
        self.set_eliminator_scores(a, b, c);
        self.gamestate = GameState::Restarting;
        self.game_timer = RESTART_TIME;
        let people: Vec<i32> = self.players.iter().filter(|(_, p)| !p.is_bot).map(|(i, _)| i as i32).collect();
        for pid in people {
            for i in 0..self.eliminator.count {
                let g = self.eliminator.groups[i];
                self.push_mission(pid, 2, i as i32, (g.eliminator << 10) + g.target + (g.protector << 20), None);
            }
        }
    }

    /// A supply drop: a grenade near one mission spot and a gun with three magazines near another, then one to three
    /// tables restocked (all three with more than 14 players ready).
    fn eliminator_drop(&mut self, ready: usize) {
        let r = rand();
        let v = crate::human::arms::calculate_spread_vector(&mut self.noise_seed, DROP_SPREAD, 0.0);
        let spot = MISSION_SPOTS[(r % 8) as usize].pos;
        let p = Vec3::new(v.x + spot.x, v.y + spot.y, v.z + spot.z);
        self.create_item(ItemKind::Grenade, p, None, IDENTITY);
        let spot = MISSION_SPOTS[(rand() % 8) as usize].pos;
        let (gun, mag) = random_gun();
        for j in 0..4 {
            let v = crate::human::arms::calculate_spread_vector(&mut self.noise_seed, DROP_GUN_SPREAD, 0.0);
            let p = Vec3::new(v.x + spot.x, 0.0 + spot.y, v.z + spot.z);
            self.create_item(if j == 0 { gun } else { mag }, p, None, IDENTITY);
        }
        let r = rand();
        let mut tables = 7;
        if ready <= 14 {
            let s = r % 3;
            tables = 1 << s;
            if ready > 7 {
                let s2 = s + 1;
                tables |= if s2 == 3 { 1 } else { 1 << s2 };
            }
        }
        for k in 0..3 {
            if tables >> k & 1 != 0 {
                self.restock_eliminator_table(k);
            }
        }
    }

    /// A table cleared along its length and stocked again with three random guns (three magazines each), a grenade
    /// and a bandage or two.
    fn restock_eliminator_table(&mut self, k: usize) {
        let base = &self.world.map.level.bases[k];
        let rot = base.frame();
        let r0 = rot[0];
        let mut p = step(base.table, r0);
        let mut q = Vec3::new(p.x, p.y - CLEAR_DROP, p.z);
        for _ in 0..CLEAR_STEPS {
            self.distance_based_item_despawning(q, CLEAR_RADIUS);
            q = step(q, r0);
        }
        for _ in 0..TABLE_GUNS {
            let (gun, mag) = random_gun();
            self.create_item(gun, p, None, rot);
            for _ in 0..3 {
                p = step(p, r0);
                self.create_item(mag, p, None, rot);
            }
            p = step(p, r0);
        }
        self.create_item(ItemKind::Grenade, p, None, rot);
        p = step(p, r0);
        let two = rand() & 1 != 0;
        self.create_item(ItemKind::Bandage, p, None, rot);
        p = step(p, r0);
        if two {
            self.create_item(ItemKind::Bandage, p, None, rot);
        }
    }

    /// distance_based_item_despawning: loose items within `radius` of `p` despawn.
    pub(crate) fn distance_based_item_despawning(&mut self, p: Vec3, radius: f32) {
        let radius_sq = radius * radius;

        for (_, item) in self.items.iter_mut() {
            if item.parent_human != -1 {
                continue;
            }

            if p.distance_squared(item.pos2) < radius_sq {
                item.despawn_time = 0;
            }
        }
    }
}

/// Hooks for checking eliminator mode against the original server.
impl Sim {
    pub fn eliminator_state(&self) -> EliminatorState {
        self.eliminator
    }

    pub fn eliminator_tick(&mut self) {
        self.logic_eliminator();
    }

    pub fn round_number(&self) -> i32 {
        self.round_number as i32
    }

    pub fn set_round_elapsed(&mut self, e: i32) {
        self.round_elapsed = e;
    }

    /// The events from `from` on as the gdb harness prints them: the type id, with a mission event's fields.
    pub fn eliminator_events(&self, from: u16) -> Vec<String> {
        let bits = |p: [f32; 3]| format!("{:x},{:x},{:x}", p[0].to_bits(), p[1].to_bits(), p[2].to_bits());
        (0..self.events.count.wrapping_sub(from))
            .filter_map(|i| self.events.get(from.wrapping_add(i)))
            .map(|e| match &e.kind {
                ServerEvent::Mission(m) => format!("21:{},{},{},{}@{}", m.player, m.kind, m.role, m.value, bits(m.pos)),
                ServerEvent::UpdateVehicleTypeColor(_) => "3".into(),
                ServerEvent::UpdatePhone(_) => "6".into(),
                ServerEvent::UpdatePlayer(_) => "7".into(),
                ServerEvent::UpdatePlayerRound(_) => "8".into(),
                ServerEvent::TeamDoor(_) => "10".into(),
                ServerEvent::UpdateCorporation(_) => "12".into(),
                ServerEvent::UpdateStock(_) => "13".into(),
                _ => "?".into(),
            })
            .collect()
    }
}
