use glam::Vec3;
use rosa_physics::rotation::RotMatrix;
use rosa_protocol::{
    CharacterCustomization, GameMode, Team,
    clientbound::game::{
        GameState, ItemKind, MenuType,
        events::{
            Event, ServerEvent, update_corporation::EventUpdateCorporation,
            update_vehicle_type_color::EventUpdateVehicleTypeColor,
        },
    },
};

use super::{Sim, item_state::ItemState};
use crate::PlayerId;

/// A corporation holds sixteen mission slots (+0x45c, 0x45c each).
pub const MISSION_SLOTS: usize = 16;
const ROUND_CORPS: usize = 3;
/// A world clock day, and the latest deadline a mission can have.
const CLOCK_DAY: i32 = 216000;
const LAST_DEADLINE: i32 = 3870000;
/// How long transaction and two-team missions run, and acquisitions.
const TRANSACTION_TIME: i32 = 414000;
const ACQUISITION_TIME: i32 = 306000;
/// The disks come in turn, black to red; gold missions are worth four times as much.
const DISK_KINDS: i32 = 6;
const GOLD: i32 = ItemKind::DiskGold as i32 - ItemKind::DiskBlack as i32;
/// The seller's price, and the buyers' base price and its steps.
const SELLER_VALUE: i32 = 100;
const BASE_VALUE: i32 = 1000;
const VALUE_STEP: i32 = 250;
/// Each round cash item is worth 250; a briefcase starts every fourth.
const CASH_VALUE: i32 = 250;
/// Steps along the table: half a unit between items, three quarters around a briefcase.
const ITEM_STEP: f32 = 0.5;
const CASE_STEP: f32 = 0.75;
/// An acquisition's reward rate per unit of value.
const RATE_SCALE: f32 = 2.5e-5;
/// At round end each member is paid their share and a flat 250.
const ROUND_WAGE: i32 = 250;
/// A share's price grows by the earnings over 50000 of itself; below 50 it is held at 50, at 500 or more (outside
/// weekly play) it splits five for one.
const EARNINGS_SCALE: f32 = 500.0;
const EARNINGS_SCALE2: f32 = 100.0;
const PRICE_FLOOR: f32 = 50.0;
const PRICE_SPLIT: f32 = 500.0;
const SPLIT_RATIO: f32 = 0.2;
const SPLIT_SHARES: i32 = 5;
/// No team has this many players.
const NO_TEAM_SIZE: i32 = 256;
/// The car chase: its key and briefcase are made at a fixed spot and taken straight into the driver's hands, and each
/// guard has a pistol and four magazines.
const CHASE_ITEMS_POS: Vec3 = Vec3::new(62.0, 10.0, 60.0);
const KEY_SLOT: usize = 4;
const CASE_SLOT: usize = 3;
const GUARD_MAGAZINES: i32 = 4;
/// The chase's target sits at no mission spot.
const CHASE_LOCATION: i32 = 0xff;
/// The randomize passes over the corporation list.
const SHUFFLES: i32 = 1024;
/// A disk left on the table rather than at a mission spot.
const NO_LOCATION: i32 = -1;

/// One mission of a corporation (0x45c bytes, the fields read so far).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Mission {
    /// +0
    pub active: bool,
    /// +4: set once the mission is decided.
    pub done: bool,
    /// +8: the mission counter when it was made, shared by the corporations of one deal.
    pub id: i32,
    /// +0xc: 1 acquisition by car chase, 2 sell a disk, 3 buy it, 4 buy it unfunded, 7 two-team sellers, 8 their buyer.
    pub kind: i32,
    /// +0x10 and +0x14: the disks of the deal.
    pub item: i32,
    pub item2: i32,
    /// +0x1c and +0x20
    pub team1: i32,
    pub team2: i32,
    /// +0x3c
    pub disk_type: i32,
    /// +0x40
    pub value: i32,
    /// +0x44: the world clock the mission runs until.
    pub deadline: i32,
    /// +0x48: the mission spot the disk waits at, or -1 for the table.
    pub location: i32,
    /// +0x4c
    pub provided_cash: i32,
    /// +0x54
    pub rate: f32,
    /// +0x58
    pub unk_58: i32,
    /// +0x5c, cleared by reset_game.
    pub unk_5c: u8,
}

/// A place in the city a sold disk can wait in a briefcase (init_mission_spots, 0x4c each).
pub struct MissionSpot {
    pub pos: Vec3,
    pub name: &'static str,
}

pub const MISSION_SPOTS: [MissionSpot; 8] = [
    MissionSpot { pos: Vec3::new(1718.0, 46.0, 1262.0), name: "Hondo Park" },
    MissionSpot { pos: Vec3::new(1778.0, 30.0, 1006.0), name: "Rio Granary" },
    MissionSpot { pos: Vec3::new(1470.0, 50.0, 1494.0), name: "Isle of Burgers" },
    MissionSpot { pos: Vec3::new(1282.0, 62.0, 1534.0), name: "Park above the Kamel Building" },
    MissionSpot { pos: Vec3::new(1674.0, 66.0, 1510.0), name: "Bobson Dugnutt Museum" },
    MissionSpot { pos: Vec3::new(1346.0, 38.0, 1298.0), name: "The Mall" },
    MissionSpot { pos: Vec3::new(1606.0, 26.0, 1166.0), name: "Gas Station" },
    MissionSpot { pos: Vec3::new(1546.0, 38.0, 1278.0), name: "Red Cube Park" },
];

/// The mission globals (0x44da5bc0 on).
#[derive(Clone, Copy, Debug, Default)]
pub struct MissionGlobals {
    /// Deals made since the reset (0x44da5bc0).
    pub counter: i32,
    /// The next disk colour (0x44da5bc4).
    pub disk_index: i32,
    /// The disks of the last deal (0x44da5ccc, 0x44da5cd0).
    pub last_disks: [i32; 2],
    /// Where the last car chase's target is (0x44da5cf0).
    pub target_location: i32,
    /// Players per corporation given missions in the last generation (0x44da5cf4).
    pub average_players: i32,
    /// The traffic car of the last car chase (0xa8f16e0).
    pub chase_car: i32,
}

fn step(p: Vec3, r: Vec3, s: f32) -> Vec3 {
    Vec3::new(r.x * s + p.x, r.y * s + p.y, r.z * s + p.z)
}

fn rand() -> i32 {
    crate::rng::rand() as i32
}

impl Sim {
    /// corp_active_mission_count: an undecided mission still running counts 1 for an acquisition, 2 otherwise.
    fn corp_active_mission_count(&self, k: usize) -> i32 {
        self.corp_state[k]
            .missions
            .iter()
            .filter(|m| m.active && !m.done && self.world_time < m.deadline)
            .map(|m| if m.kind <= 1 { 1 } else { 2 })
            .sum()
    }

    /// generate_round_missions: the round corporations with players and no missions running (or one, at the turn of a
    /// day) shuffled, then dealt out in groups to transactions, two-team deals and acquisitions.
    pub(crate) fn generate_round_missions(&mut self) {
        self.missions.average_players = 0;
        for c in &mut self.corp_state[..ROUND_CORPS] {
            c.spent = 0;
        }
        let day_start = self.world_time % CLOCK_DAY == 0;
        let (mut list, mut total) = (Vec::new(), 0);
        for k in 0..ROUND_CORPS {
            let players = self.corp_state[k].player_count;
            if players <= 0 {
                continue;
            }
            let active = self.corp_active_mission_count(k);
            if active <= 0 || (active == 1 && day_start) {
                list.push(k as i32);
                total += players;
                self.missions.average_players = total;
            }
        }
        if list.is_empty() {
            self.missions.average_players = 1;
            return;
        }
        self.missions.average_players = (total / list.len() as i32).max(1);
        let n = list.len() as i32;
        if n > 1 {
            for _ in 0..SHUFFLES {
                let a = (rand() % n) as usize;
                let b = (rand() % n) as usize;
                list.swap(a, b);
            }
        }
        while !list.is_empty() {
            let size = self.deal_round_missions(&list);
            self.stats.missions += 1;
            if list.len() as i32 - size <= 0 {
                break;
            }
            list.drain(..size as usize);
        }
    }

    /// One deal of generate_round_missions for the head of `list`; the number of corporations it took.
    fn deal_round_missions(&mut self, list: &[i32]) -> i32 {
        let n = list.len() as i32;
        let couple_first = if n <= 1 {
            false
        } else if rand() & 7 <= 5 {
            return self.deal_transaction(list);
        } else if n == 3 {
            if rand() & 3 == 0 {
                self.generate_twoteam_missions(&list[..3]);
                return 3;
            }
            true
        } else if rand() & 3 > 1 || n <= 3 {
            let many = n > 2;
            if rand() & 3 == 0 && many {
                return self.couple_or_single(list);
            }
            return self.chase_or_single(list, n);
        } else {
            if rand() & 3 == 0 {
                return self.couple_or_single(list);
            }
            let size = if rand() & 3 == 0 { 1 } else { 3 };
            self.create_mission_acquisition(&list[..size]);
            return size as i32;
        };
        rand();
        if rand() & 3 == 0 && couple_first { self.couple_or_single(list) } else { self.chase_or_single(list, n) }
    }

    /// A transaction for two, three or all of `list`, or a two-team deal for three.
    fn deal_transaction(&mut self, list: &[i32]) -> i32 {
        let n = list.len() as i32;
        let size = if rand() & 1 == 0 && n > 3 {
            if rand() & 15 == 0 { 2 } else { 0 }
        } else if rand() & 15 == 0 {
            2
        } else if n == 3 {
            0
        } else {
            n
        };
        if size != 0 {
            self.generate_transaction_missions(&list[..size as usize]);
            return size;
        }
        if rand() & 3 != 0 {
            self.generate_transaction_missions(&list[..3]);
        } else {
            self.generate_twoteam_missions(&list[..3]);
        }
        3
    }

    /// An acquisition for `size` corporations, or one in four times (with more than one to pick from) for the first.
    fn chase_or_single(&mut self, list: &[i32], size: i32) -> i32 {
        let size = if rand() & 3 != 0 || list.len() <= 1 { size } else { 1 };
        self.create_mission_acquisition(&list[..size as usize]);
        size
    }

    /// An acquisition for the first two corporations, or one in four times the first alone.
    fn couple_or_single(&mut self, list: &[i32]) -> i32 {
        let size = if rand() & 3 != 0 { 2 } else { 1 };
        self.create_mission_acquisition(&list[..size as usize]);
        size
    }

    /// Takes the next disk colour, black to red and over again.
    fn next_disk(&mut self) -> i32 {
        let old = self.missions.disk_index;
        self.missions.disk_index = if old + 1 > DISK_KINDS - 1 { 0 } else { old + 1 };
        old
    }

    fn deadline(&self, time: i32) -> i32 {
        (self.world_time + time).min(LAST_DEADLINE)
    }

    /// The table frame of a corporation and the place half a unit along it the items are laid out from.
    fn table_start(&self, k: usize) -> (rosa_physics::rotation::RotMatrix, Vec3) {
        let base = &self.world.map.level.bases[k];
        let rot = base.frame();
        (rot, step(base.table, rot[0], ITEM_STEP))
    }

    /// The round cash of a buyer: `count` bundles along the table, a briefcase before every fourth, each taken from
    /// the corporation's spending.
    fn lay_out_cash(&mut self, k: usize, rot: rosa_physics::rotation::RotMatrix, mut p: Vec3, count: i32) {
        let r = rot[0];
        for j in 0..count {
            if j & 3 == 0 {
                p = step(p, r, CASE_STEP);
                self.create_item(ItemKind::Briefcase, p, None, rot);
                p = step(p, r, CASE_STEP);
            }
            self.create_item(ItemKind::CashRound, p, None, rot);
            self.corp_state[k].spent -= CASH_VALUE;
            p = step(p, r, ITEM_STEP);
        }
    }

    /// A disk on a corporation's table with a briefcase beside it.
    fn lay_out_disk(&mut self, k: usize, disk: ItemKind, value: i32, rot: rosa_physics::rotation::RotMatrix, p: Vec3) -> i32 {
        let id = self.create_item(disk, p, None, rot).map_or(-1, |i| i as i32);
        self.corp_state[k].spent -= value;
        let p = step(step(p, rot[0], ITEM_STEP), rot[0], CASE_STEP);
        self.create_item(ItemKind::Briefcase, p, None, rot);
        id
    }

    /// The first free mission slot of a corporation, its id set and marked active.
    fn open_mission(&mut self, k: usize) -> Option<usize> {
        let slot = self.corp_state[k].missions.iter().position(|m| !m.active)?;
        let m = &mut self.corp_state[k].missions[slot];

        m.id = self.missions.counter;
        m.active = true;

        Some(slot)
    }

    fn item_kind(&self, id: i32) -> i32 {
        usize::try_from(id).ok().and_then(|i| self.items.get(i)).map_or(0, |i| i.item_type as i32)
    }

    /// generate_transaction_missions: the first corporation sells a disk (on its table, or three times in four in a
    /// briefcase at a mission spot), one or two others are given round cash to buy it and the rest may buy it unfunded.
    fn generate_transaction_missions(&mut self, list: &[i32]) {
        let n = list.len();
        let twice = (rand() & 3 == 0) as usize + 1;
        let funded = if twice < n { twice } else { 1 };
        let deadline = self.deadline(TRANSACTION_TIME);
        let colour = self.next_disk();
        let disk = ItemKind::try_from((ItemKind::DiskBlack as i32 + colour) as u8).unwrap();
        let location = if rand() & 3 != 0 { rand() % 8 } else { NO_LOCATION };
        let values: Vec<i32> = (0..n)
            .map(|i| {
                if i == 0 {
                    return SELLER_VALUE;
                }

                let mut v = rand() % 5 * VALUE_STEP + BASE_VALUE;
                if funded < i {
                    v += rand() % 5 * VALUE_STEP;
                }

                if colour == GOLD { v << 2 } else { v }
            })
            .collect();

        for (i, &c) in list.iter().enumerate() {
            let k = c as usize;
            let (rot, p) = self.table_start(k);
            if i == 0 {
                if location == NO_LOCATION {
                    self.missions.last_disks[0] = self.lay_out_disk(k, disk, values[0], rot, p);
                } else {
                    let p = MISSION_SPOTS[location as usize].pos;
                    let id = self.create_item(disk, p, None, rot);
                    self.corp_state[k].spent -= values[0];
                    self.missions.last_disks[0] = id.map_or(-1, |i| i as i32);
                    if let Some(case) = self.create_item(ItemKind::Briefcase, p, None, rot)
                        && let Some(id) = id
                    {
                        self.items.get_mut(case).unwrap().item_type = ItemKind::BriefcaseOpen;
                        super::items::attach_child(&mut self.items, &self.item_types, case, id);
                        self.items.get_mut(case).unwrap().item_type = ItemKind::Briefcase;
                    }
                }
            } else if i <= funded {
                let mut count = rand() % 3 + 2;
                if colour == GOLD {
                    count += rand() % 3 + 2;
                }
                self.lay_out_cash(k, rot, p, count);
            }
        }
        let item = self.missions.last_disks[0];
        let disk_type = self.item_kind(item);

        for (i, &c) in list.iter().enumerate() {
            let k = c as usize;
            let Some(slot) = self.open_mission(k) else { continue };
            let spent = self.corp_state[k].spent;
            let m = &mut self.corp_state[k].missions[slot];
            
            m.item = item;
            m.disk_type = disk_type;
            m.value = values[i];
            m.deadline = deadline;
            m.team1 = list[0];

            (m.kind, m.location, m.provided_cash) = match i {
                0 => (2, location, 0),
                _ if i <= funded => (3, NO_LOCATION, -spent),
                _ => (4, NO_LOCATION, 0),
            };

            self.push_mission_event(k, slot);
        }

        self.missions.counter += 1;
    }

    /// generate_twoteam_missions: the first two corporations each sell a disk to the third, which gets round cash.
    fn generate_twoteam_missions(&mut self, list: &[i32]) {
        let n = list.len();
        let deadline = self.deadline(TRANSACTION_TIME);
        let colour = self.next_disk();
        let disk = ItemKind::try_from((ItemKind::DiskBlack as i32 + colour) as u8).unwrap();
        let values: Vec<i32> = (0..n)
            .map(|i| {
                if i <= 1 {
                    return SELLER_VALUE;
                }
                let a = rand() % 5 * VALUE_STEP;
                let b = rand() % 5 * VALUE_STEP;
                let v = a + b + BASE_VALUE;
                (if colour == GOLD { v * 4 } else { v }) * 2
            })
            .collect();
        for (i, &c) in list.iter().enumerate() {
            let k = c as usize;
            let (rot, p) = self.table_start(k);
            if i <= 1 {
                self.missions.last_disks[i] = self.lay_out_disk(k, disk, values[i], rot, p);
            } else if i == 2 {
                let mut count = rand() % 5 + 4;
                if colour == GOLD {
                    count += rand() % 3 + 2;
                }
                self.lay_out_cash(k, rot, p, count);
            }
        }
        let [item, item2] = self.missions.last_disks;
        let disk_type = self.item_kind(item);
        for (i, &c) in list.iter().enumerate() {
            let k = c as usize;
            let Some(slot) = self.open_mission(k) else { continue };
            let spent = self.corp_state[k].spent;
            let m = &mut self.corp_state[k].missions[slot];
            m.item = item;
            m.item2 = item2;
            m.disk_type = disk_type;
            m.value = values[i];
            m.deadline = deadline;
            if i <= 1 {
                m.kind = 7;
                m.team1 = list[2];
                m.provided_cash = 0;
            } else {
                if i == 2 {
                    m.kind = 8;
                }
                m.team1 = list[0];
                m.team2 = list[1];
                m.provided_cash = if i == 2 { -spent } else { 0 };
            }
            self.push_mission_event(k, slot);
        }
        self.missions.counter += 1;
    }

    /// create_mission_aquisition (car chase): a disk carried through the streets in a guarded traffic car, every
    /// corporation of the group racing to take it.
    fn create_mission_acquisition(&mut self, list: &[i32]) {
        let deadline = self.deadline(ACQUISITION_TIME);
        let colour = self.next_disk();
        if !self.world.map.streets.streets.is_empty() {
            let disk = ItemKind::try_from((ItemKind::DiskBlack as i32 + colour) as u8).unwrap();
            self.create_round_traffic_car(disk, deadline);
        }
        let item = self.missions.last_disks[0];
        let disk_type = self.item_kind(item);
        for &c in list {
            let k = c as usize;
            let Some(slot) = self.open_mission(k) else { continue };
            let mut value = rand() % 5 * VALUE_STEP + BASE_VALUE;
            if colour == GOLD {
                value <<= 2;
            }
            let location = self.missions.target_location;
            let m = &mut self.corp_state[k].missions[slot];
            m.kind = 1;
            m.item = item;
            m.disk_type = disk_type;
            m.value = value;
            m.deadline = deadline;
            m.unk_58 = 0;
            m.provided_cash = 0;
            m.location = location;
            m.rate = value as f32 * RATE_SCALE;
            self.push_mission_event(k, slot);
        }
        self.missions.counter += 1;
    }

    /// create_round_traffic_car: a traffic car on a random street, routed to another and spawned as a vehicle at once,
    /// with a mission bot driving it holding its key and a briefcase with the disk, and one to three armed guards.
    pub(crate) fn create_round_traffic_car(&mut self, disk: ItemKind, deadline: i32) {
        use crate::traffic::{random_street, route::plan_route, spawn::create_traffic_car};
        use rosa_physics::rotation::{IDENTITY, rotate_orientation};
        let slot = rand() & 1;
        let streets = &self.world.map.streets;
        let street = if streets.streets.is_empty() { 0 } else { random_street(streets) };
        let progress = (rand() & 255) as f32 * (1.0 / 256.0);
        let car = create_traffic_car(&mut self.traffic, &self.world.map, &self.vehicle_types, 0, 0, street, slot, 1, progress);
        self.missions.chase_car = car as i32;
        let streets = &self.world.map.streets;
        let to = if streets.streets.is_empty() { 0 } else { random_street(streets) };
        let to_slot = rand() & 1;
        let c = &mut self.traffic.cars[car];
        let from = c.street;
        plan_route(c, streets, from, slot, to, to_slot);
        c.is_bot = crate::traffic::PARKED;
        let mut rot = IDENTITY;
        let axis = rot[1];
        rotate_orientation(&mut rot, axis, std::f32::consts::FRAC_PI_2);
        let axis = rot[1];
        rotate_orientation(&mut rot, axis, c.yaw);
        c.rot = rot;
        let pos = super::traffic::vehicle_pos(c, &self.vehicle_types);
        let (vel, kind, color) = (c.vel, c.kind, c.color);
        let vid = crate::vehicle::spawn_vehicle(&mut self.vehicles, &mut self.bodies, &self.vehicle_types, kind, color, pos, rot, Some(vel));
        self.traffic.cars[car].vehicle = vid.map_or(-1, |v| v as i32);
        let Some(vid) = vid else { return };
        let e = EventUpdateVehicleTypeColor { vehicle_id: vid as i32, vehicle_type: kind as u8, vehicle_color: color as u8 };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::UpdateVehicleTypeColor(e) });
        let v = self.vehicles.get_mut(vid).unwrap();
        v.traffic_car = car as i32;
        let (vpos, vrot) = (v.pos, v.rot);
        let Some((driver, driver_human)) = self.create_mission_bot(deadline, vpos, vrot, vid, 0) else { return };
        if let Some(key) = self.create_item(ItemKind::Key, CHASE_ITEMS_POS, None, IDENTITY) {
            self.items.get_mut(key).unwrap().state = ItemState::Key { vehicle: Some(vid) };
            if let Some(h) = driver_human {
                self.link_to_slot(h, key, KEY_SLOT);
            }
        }
        let case = self.create_item(ItemKind::Briefcase, CHASE_ITEMS_POS, None, IDENTITY);
        if let Some(case) = case {
            self.items.get_mut(case).unwrap().item_type = ItemKind::BriefcaseOpen;
        }
        let id = self.create_item(disk, CHASE_ITEMS_POS, None, IDENTITY);
        self.missions.last_disks[0] = id.map_or(-1, |i| i as i32);
        self.missions.target_location = CHASE_LOCATION;
        if let Some(case) = case {
            if let Some(id) = id {
                super::items::attach_child(&mut self.items, &self.item_types, case, id);
            }
            self.items.get_mut(case).unwrap().item_type = ItemKind::Briefcase;
            if let Some(h) = driver_human {
                self.link_to_slot(h, case, CASE_SLOT);
            }
        }
        if let Some(p) = self.players.get_mut(driver.idx()) {
            p.team = Team::Mission;
            p.bot.unk_2d24 = 1;
            p.bot.waypoint = 0;
            p.bot.waypoint_count = 0;
        }
        self.create_guard(deadline, vpos, vrot, vid, 1);
        if rand() & 1 == 0 {
            self.create_guard(deadline, vpos, vrot, vid, 2);
        }
        if rand() & 1 == 0 {
            self.create_guard(deadline, vpos, vrot, vid, 3);
        }
    }

    fn create_guard(&mut self, deadline: i32, pos: Vec3, rot: RotMatrix, vehicle: usize, seat: usize) {
        if let Some((_, Some(h))) = self.create_mission_bot(deadline, pos, rot, vehicle, seat) {
            self.give_weapon(h, ItemKind::Pistol, GUARD_MAGAZINES);
        }
    }

    /// A mission bot of the chase's team seated in its vehicle: the player and its human.
    fn create_mission_bot(&mut self, deadline: i32, pos: Vec3, rot: RotMatrix, vehicle: usize, seat: usize) -> Option<(PlayerId, Option<usize>)> {
        let pid = self.create_player()?;
        let counter = self.missions.counter;
        let p = self.players.get_mut(pid.idx()).unwrap();
        p.team = Team::Mission;
        p.username.clear();
        p.bot_deadline = deadline;
        p.bot_mission = counter;
        p.is_bot = true;
        p.customization.gender = 1;
        p.customization.model = 2;
        p.customization.suit_color = 1;
        let h = self.spawn_human(pos, &rot, Some(pid));
        let p = self.players.get_mut(pid.idx()).unwrap();
        p.human = h;
        let e = p.make_update_player_event(self.tick);
        self.events.push(e);
        if let Some(hu) = h.and_then(|h| self.humans.get_mut(h)) {
            hu.vehicle = Some(vehicle);
            hu.seat = seat;
        }
        Some((pid, h))
    }

    /// create_player: a new player with 50000 money and a random look; in a round's intermission it starts in the
    /// lobby menu.
    pub(crate) fn create_player(&mut self) -> Option<PlayerId> {
        rand();
        let head = rand() % 5;
        let mut skin = rand() % 6;
        if rand() & 3 != 3 {
            skin = 2;
        }
        let hair_color = rand() % 6;
        let hair_style = rand() % 9;
        let eye_color = rand() % 8;
        let customization = CharacterCustomization {
            gender: 0,
            head: head as u8,
            skin: skin as u8,
            hair_color: hair_color as u8,
            hair_style: hair_style as u8,
            eye_color: eye_color as u8,
            model: 0,
            necklace: 0,
            suit_color: 0,
            tie_color: 0,
        };
        let lobby = self.gamestate == GameState::Intermission && self.gamemode == GameMode::Round;
        let menu = if lobby { MenuType::Lobby } else { MenuType::Empty };
        let entry = self.players.vacant_entry();
        let pid = PlayerId(entry.key() as u32);
        entry.insert(crate::player::Player::new_bot(pid, customization, menu));
        Some(pid)
    }

    /// give_weapon: a gun in the human's right hand, a magazine in the left and the rest two to a slot from slot 3.
    pub(crate) fn give_weapon(&mut self, h: usize, gun: ItemKind, magazines: i32) {
        let Some((pos, rot)) = self.humans.get(h).map(|hu| (hu.bones[0].pos, hu.bones[0].rot)) else { return };
        let mag = ItemKind::try_from(gun as u8 + 1).unwrap();
        if let Some(id) = self.create_item(gun, pos, None, rot) {
            self.link_to_slot(h, id, 0);
        }
        if let Some(id) = self.create_item(mag, pos, None, rot) {
            self.link_to_slot(h, id, 1);
        }
        for i in 0..magazines - 1 {
            if let Some(id) = self.create_item(mag, pos, None, rot) {
                self.link_to_slot(h, id, (i >> 1) as usize + 3);
            }
        }
    }

    /// create_event_update_corporation
    pub(crate) fn push_mission_event(&mut self, k: usize, slot: usize) {
        let m = &self.corp_state[k].missions[slot];
        let e = EventUpdateCorporation {
            a: m.active as i32 + ((slot as i32) << 1) + ((k as i32) << 4) + (m.kind << 8) + (m.disk_type << 16),
            b: m.team1 + (m.team2 << 4) + (m.value << 8),
            c: m.deadline / 60 + ((m.location + 1) << 16),
            d: m.provided_cash,
        };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::UpdateCorporation(e) });
    }
}

impl Sim {
    /// round_complete: each round corporation earns the round cash in its base and the value of the missions whose
    /// disks are there, less what its missions were given; its members share the earnings (larger teams split them
    /// further with bonusratio on) and its share price moves with them.
    pub(crate) fn round_complete(&mut self) {
        let divisors: [i32; ROUND_CORPS] = if self.round_cfg.bonusratio != 0 {
            let counts: [i32; ROUND_CORPS] = std::array::from_fn(|k| {
                let team = rosa_protocol::Team::CORPORATIONS[k];
                self.players.iter().filter(|(_, p)| p.team == team).count() as i32
            });
            let least = counts.iter().fold(NO_TEAM_SIZE, |least, &n| if n < least && n > 0 { n } else { least });
            counts.map(|n| if n > 0 { n / least } else { 1 })
        } else {
            [1; ROUND_CORPS]
        };
        let weekly = self.round_cfg.weekly;
        for k in 0..ROUND_CORPS {
            let base = &self.world.map.level.bases[k];
            let (lo, hi) = (base.interior_min, base.interior_max);
            let inside = |p: Vec3| p.x >= lo.x && hi.x + 0.0 > p.x && p.z >= lo.z && hi.z + 0.0 > p.z;
            let pos = |id: i32| usize::try_from(id).ok().and_then(|i| self.items.get(i)).map(|i| i.pos2);
            let mut earnings = self.corp_state[k].spent;
            earnings += self.items.iter().filter(|(_, i)| i.item_type == ItemKind::CashRound && inside(i.pos2)).count() as i32 * CASH_VALUE;
            for m in self.corp_state[k].missions.iter().filter(|m| m.active && m.item != -1) {
                if !pos(m.item).is_some_and(inside) {
                    continue;
                }
                if (7..=9).contains(&m.kind) && m.item2 != -1 && !pos(m.item2).is_some_and(inside) {
                    continue;
                }
                earnings += m.value;
            }
            let team = rosa_protocol::Team::CORPORATIONS[k];
            for (_, p) in self.players.iter_mut().filter(|(_, p)| p.team == team) {
                let share = earnings / divisors[k];
                p.money += share + ROUND_WAGE;
                if weekly
                    && p.account_id != u32::MAX
                    && let Some(a) = self.saved_accounts.get_player_data(p.account_id)
                {
                    a.money = (a.money as i32 + share) as u32;
                }
            }
            self.corp_state[k].funds = earnings;
            let corp = &mut self.corporations[k];
            corp.price *= earnings as f32 / EARNINGS_SCALE / EARNINGS_SCALE2 + 1.0;
            if PRICE_FLOOR > corp.price {
                corp.price = PRICE_FLOOR;
            } else if !weekly && corp.price >= PRICE_SPLIT {
                corp.price *= SPLIT_RATIO;
                for (_, p) in self.players.iter_mut().filter(|(_, p)| p.team == team) {
                    p.stocks *= SPLIT_SHARES;
                }
            }
        }
        let e = super::economy::stock_event(&self.corporations, self.tick);
        self.events.push(e);
        for (_, p) in self.players.iter() {
            let e = p.make_update_round_event(self.tick);
            self.events.push(e);
        }
    }
}

/// Hooks for checking the missions against the original server.
impl Sim {
    pub fn run_generate_round_missions(&mut self) {
        self.generate_round_missions();
    }

    pub fn run_round_complete(&mut self) {
        self.round_complete();
    }

    pub fn set_round_settings(&mut self, bonusratio: i32, weekly: bool) {
        self.round_cfg.bonusratio = bonusratio;
        self.round_cfg.weekly = weekly;
    }

    pub fn share_price(&self, k: usize) -> f32 {
        self.corporations[k].price
    }

    pub fn set_share_price(&mut self, k: usize, price: f32) {
        self.corporations[k].price = price;
    }

    /// The type ids of the events from `from` on, with a stock update's three ints.
    pub fn event_summary(&self, from: u16) -> Vec<(i32, Option<[i32; 3]>)> {
        (0..self.events.count.wrapping_sub(from))
            .filter_map(|i| self.events.get(from.wrapping_add(i)))
            .map(|e| match &e.kind {
                ServerEvent::UpdateStock(s) => (13, Some(s.prices)),
                ServerEvent::UpdatePlayerRound(_) => (8, None),
                ServerEvent::UpdatePlayer(_) => (7, None),
                ServerEvent::UpdateCorporation(_) => (12, None),
                ServerEvent::UpdateVehicleTypeColor(_) => (3, None),
                _ => (-1, None),
            })
            .collect()
    }

    pub fn traffic_cars(&self) -> &[crate::traffic::TrafficCar] {
        &self.traffic.cars
    }

    /// Each vehicle: its id, traffic car and body position.
    pub fn vehicle_list(&self) -> Vec<(usize, i32, Vec3)> {
        self.vehicles.iter().map(|(i, v)| (i, v.traffic_car, self.bodies.get(v.body).map_or(Vec3::ZERO, |b| b.pos))).collect()
    }

    pub fn player_ids(&self) -> Vec<PlayerId> {
        let mut ids: Vec<PlayerId> = self.players.iter().map(|(i, _)| PlayerId(i as u32)).collect();
        ids.sort_by_key(|p| p.0);
        ids
    }

    pub fn human_ids(&self) -> Vec<usize> {
        self.humans.iter().map(|(i, _)| i).collect()
    }

    pub fn mission_globals(&self) -> MissionGlobals {
        self.missions
    }

    pub fn set_world_time(&mut self, t: i32) {
        self.world_time = t;
    }

    pub fn world_time(&self) -> i32 {
        self.world_time
    }

    /// The events from `from` on that are corporation updates, as their four ints.
    pub fn corporation_events(&self, from: u16) -> Vec<[i32; 4]> {
        (0..self.events.count.wrapping_sub(from))
            .filter_map(|i| self.events.get(from.wrapping_add(i)))
            .filter_map(|e| match &e.kind {
                ServerEvent::UpdateCorporation(c) => Some([c.a, c.b, c.c, c.d]),
                _ => None,
            })
            .collect()
    }

    pub fn event_count(&self) -> u16 {
        self.events.count
    }
}
