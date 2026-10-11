use glam::Vec3;
use rosa_physics::rotation::RotMatrix;
use rosa_protocol::{
    CharacterCustomization, Team,
    clientbound::game::{
        GameState, ItemKind, MenuType,
        events::{
            Event, ServerEvent, mission_text::EventMissionText, update_corporation::EventUpdateCorporation,
            update_vehicle_type_color::EventUpdateVehicleTypeColor,
        },
    },
};

use super::Sim;
use crate::{PlayerId, items::state::ItemState};

/// A corporation holds sixteen mission slots (+0x45c, 0x45c each).
pub const MISSION_SLOTS: usize = 16;
/// A mission text event carries 64 bytes of the text.
const TEXT_LINE: usize = 64;
/// The car chase: its key and briefcase are made at a fixed spot and taken straight into the driver's hands, and each
/// guard has a pistol and four magazines.
const CHASE_ITEMS_POS: Vec3 = Vec3::new(62.0, 10.0, 60.0);
const KEY_SLOT: usize = 4;
const CASE_SLOT: usize = 3;
const GUARD_MAGAZINES: i32 = 4;
/// The chase's target sits at no mission spot.
const CHASE_LOCATION: i32 = 255;

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
    /// +0x5c: the line settle_mission_result writes, kept as whether it expired and the rating it gave (None
    /// before, and after reset_game clears it).
    pub result: Option<(bool, i32)>,
}

pub const MISSION_SPOTS: [Vec3; 8] = [
    // Hondo Park
    Vec3::new(1718.0, 46.0, 1262.0),
    // Rio Granary
    Vec3::new(1778.0, 30.0, 1006.0),
    // Isle of Burgers
    Vec3::new(1470.0, 50.0, 1494.0),
    // Park above the Kamel Building
    Vec3::new(1282.0, 62.0, 1534.0),
    // Bobson Dugnutt Museum
    Vec3::new(1674.0, 66.0, 1510.0),
    // The Mall
    Vec3::new(1346.0, 38.0, 1298.0),
    // Gas Station
    Vec3::new(1606.0, 26.0, 1166.0),
    // Red Cube Park
    Vec3::new(1546.0, 38.0, 1278.0),
];

/// settle_mission_result: a failed acquisition costs 10 rating; a seller gains 50 plus 10 on success when paid at
/// least what it was given, and up to 10 back on success when short.
const FAILED_RATING: i32 = -10;
const SELLER_RATING: i32 = 50;
const SUCCESS_RATING: i32 = 10;
/// Each rating point a member gains also pays 10.
const RATING_MONEY: i32 = 10;

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

impl Sim {
    /// create_round_traffic_car: a traffic car on a random street, routed to another and spawned as a vehicle at once,
    /// with a mission bot driving it holding its key and a briefcase with the disk, and one to three armed guards.
    pub(crate) fn create_round_traffic_car(&mut self, disk: ItemKind, deadline: i32) {
        use rosa_physics::rotation::IDENTITY;
        let (_, Some(vid)) = self.spawn_chase_car(None) else { return };
        let v = self.vehicles.get(vid).unwrap();
        let (vpos, vrot) = (v.pos, v.rot);
        let counter = self.missions.counter;
        let Some((driver, driver_human)) = self.create_mission_bot(deadline, Some(counter), vpos, vrot, vid, 0) else { return };
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
            p.bot.waypoint = 0;
            p.bot.waypoint_count = 0;
        }
        self.create_guards(deadline, Some(counter), vpos, vrot, vid);
    }

    /// A town car (create_traffic_car on a random street) routed to another street, at least `min_distance` away
    /// when given (1024 tries), and spawned as a vehicle at once: the car and its vehicle.
    pub(crate) fn spawn_chase_car(&mut self, min_distance: Option<f32>) -> (usize, Option<usize>) {
        use crate::traffic::{random_street, route::plan_route, spawn::create_traffic_car};
        use rosa_physics::rotation::{IDENTITY, rotate_orientation};
        let slot = rand::random_range(0..2);
        let streets = &self.world.map.streets;
        let street = if streets.streets.is_empty() { 0 } else { random_street(streets) };
        let progress = rand::random_range(0..256) as f32 * (1.0 / 256.0);
        let car = create_traffic_car(
            &mut self.traffic,
            &self.world.map,
            &self.vehicle_types,
            rosa_protocol::clientbound::game::VehicleKind::TownCar,
            0,
            street,
            slot,
            1,
            progress,
        );
        self.missions.chase_car = car as i32;
        let streets = &self.world.map.streets;
        let mut to = if streets.streets.is_empty() { 0 } else { random_street(streets) };
        if let Some(min) = min_distance {
            let corner = |s: i32| usize::try_from(s).ok().and_then(|s| streets.streets.get(s)).map_or(Vec3::ZERO, |s| s.bounds_min);
            let from = corner(self.traffic.cars[car].street);
            for _ in 0..1024 {
                let d = from - corner(to);
                if d.length() >= min {
                    break;
                }
                if !streets.streets.is_empty() {
                    to = random_street(streets);
                }
            }
        }
        let to_slot = rand::random_range(0..2);
        let c = &mut self.traffic.cars[car];
        let from = c.street;
        plan_route(c, streets, from, slot, to, to_slot);
        c.is_bot = crate::traffic::PARKED;
        let mut rot = IDENTITY;
        let axis = rot[1];
        rotate_orientation(&mut rot, axis, 90.0_f32.to_radians());
        let axis = rot[1];
        rotate_orientation(&mut rot, axis, c.yaw);
        c.rot = rot;
        let pos = super::traffic::vehicle_pos(c, &self.vehicle_types);
        let (vel, kind, color) = (c.vel, c.kind, c.color);
        let vid = crate::vehicle::spawn_vehicle(&mut self.vehicles, &mut self.bodies, &self.vehicle_types, kind, color, pos, rot, Some(vel));
        self.traffic.cars[car].vehicle = vid.map_or(-1, |v| v as i32);
        let Some(vid) = vid else { return (car, None) };
        let e = EventUpdateVehicleTypeColor { vehicle_id: vid as i32, vehicle_type: kind, vehicle_color: color as u8 };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::UpdateVehicleTypeColor(e) });
        self.vehicles.get_mut(vid).unwrap().traffic_car = car as i32;
        (car, Some(vid))
    }

    /// The chase car's guards: one in the front passenger seat, each back seat taken at even odds.
    pub(crate) fn create_guards(&mut self, deadline: i32, mission: Option<i32>, pos: Vec3, rot: RotMatrix, vehicle: usize) {
        self.create_guard(deadline, mission, pos, rot, vehicle, 1);
        if !rand::random::<bool>() {
            self.create_guard(deadline, mission, pos, rot, vehicle, 2);
        }
        if !rand::random::<bool>() {
            self.create_guard(deadline, mission, pos, rot, vehicle, 3);
        }
    }

    fn create_guard(&mut self, deadline: i32, mission: Option<i32>, pos: Vec3, rot: RotMatrix, vehicle: usize, seat: usize) {
        if let Some((_, Some(h))) = self.create_mission_bot(deadline, mission, pos, rot, vehicle, seat) {
            self.give_weapon(h, ItemKind::Pistol, GUARD_MAGAZINES);
        }
    }

    /// A mission bot of the chase's team seated in its vehicle (with the round's mission counter when given): the
    /// player and its human.
    pub(crate) fn create_mission_bot(
        &mut self,
        deadline: i32,
        mission: Option<i32>,
        pos: Vec3,
        rot: RotMatrix,
        vehicle: usize,
        seat: usize,
    ) -> Option<(PlayerId, Option<usize>)> {
        let pid = self.create_player()?;
        let p = self.players.get_mut(pid.idx()).unwrap();

        p.team = Team::Mission;
        p.username.clear();
        p.bot_deadline = deadline;

        if let Some(m) = mission {
            p.bot_mission = m;
        }

        p.is_bot = true;
        p.customization.gender = 1;
        p.customization.model = 2;
        p.customization.suit_color = 1;

        let new_human = self.spawn_human(pos, &rot, Some(pid));
        let bot_player = self.players.get_mut(pid.idx()).unwrap();

        bot_player.human = new_human;
        self.update_player_event(pid);

        if let Some(hu) = new_human.and_then(|h| self.humans.get_mut(h)) {
            hu.vehicle = Some(vehicle);
            hu.seat = seat;
        }
        
        Some((pid, new_human))
    }

    /// create_player: a new player with 50000 money and a random look; in a round's intermission it starts in the
    /// lobby menu.
    pub(crate) fn create_player(&mut self) -> Option<PlayerId> {
        let head = rand::random_range(0..5);
        let mut skin = rand::random_range(0..6);
        if rand::random_ratio(3, 4) {
            skin = 2;
        }
        let hair_color = rand::random_range(0..6);
        let hair_style = rand::random_range(0..9);
        let eye_color = rand::random_range(0..8);
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
        let lobby = self.gamestate == GameState::Intermission && self.rules.bot_lobby;
        let menu = if lobby { MenuType::Lobby } else { MenuType::Empty };
        let pid = PlayerId(self.players.next_key() as u32);
        self.players.insert(crate::player::Player::new_bot(pid, customization, menu));
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

    /// mission_broadcast_text_lines: the mission's text (its result line) to the corporation's team, 64 bytes a line,
    /// with one line more than the text and its terminator fill.
    pub(crate) fn push_mission_text(&mut self, k: usize, slot: usize) {
        let mut text = self.corp_state[k].missions[slot].result_text(slot).into_bytes();
        let lines = (text.len() + 1) / TEXT_LINE + 1;
        text.resize(lines * TEXT_LINE, 0);
        for (line, chunk) in text.chunks(TEXT_LINE).enumerate() {
            let e = EventMissionText { corporation: k as i32, mission: slot as i32, line: line as i32, text: chunk.to_vec() };
            self.events.push(Event { tick_created: self.tick, kind: ServerEvent::MissionText(e) });
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

impl Mission {
    /// The result line: "Acquisition(n): Complete   Rating:  r" or "Transaction(n): Expired    Rating:  r".
    pub(crate) fn result_text(&self, slot: usize) -> String {
        let Some((expired, rating)) = self.result else { return String::new() };
        let name = if (self.kind as u32) <= 1 { "Acquisition" } else { "Transaction" };
        let state = if expired { "Expired   " } else { "Complete  " };
        format!("{name}({}): {state} Rating:  {rating}", slot + 1)
    }
}

impl Sim {
    /// despawn_missions_by_type: every undecided mission of the deal lets go of its disk, which despawns.
    pub(crate) fn despawn_missions_by_id(&mut self, id: i32) {
        for k in 0..self.corp_state.len() {
            for slot in 0..MISSION_SLOTS {
                let m = self.corp_state[k].missions[slot];
                if !m.active || m.done || m.id != id {
                    continue;
                }
                if let Some(item) = usize::try_from(m.item).ok().and_then(|i| self.items.get_mut(i)) {
                    item.despawn_time = 0;
                }
                self.corp_state[k].missions[slot].item = -1;
            }
        }
    }

    /// settle_mission_result: the corporation's members gain the mission's rating (an acquisition its value on
    /// success, a transaction the cash against what it was given) and ten times it in money when positive, and the
    /// mission is decided.
    pub(crate) fn settle_mission_result(&mut self, k: usize, slot: usize, success: bool, cash: i32) {
        let m = self.corp_state[k].missions[slot];
        let tenth = cash / 10;
        let rating = match m.kind {
            0 | 1 => (if success { m.value / 10 } else { FAILED_RATING }) + tenth,
            2 => {
                let r = tenth - m.provided_cash / 10;
                if r >= 0 {
                    r + SELLER_RATING + if success { SUCCESS_RATING } else { 0 }
                } else if !success {
                    r
                } else if r >= FAILED_RATING {
                    r + SUCCESS_RATING
                } else {
                    FAILED_RATING
                }
            }
            _ => tenth - m.provided_cash / 10 + if success { m.value / 10 } else { 0 },
        };
        let team = Team::CORPORATIONS[k];
        for (_, p) in self.players.iter_mut().filter(|(_, p)| p.team == team) {
            if rating > 0 {
                p.money += rating * RATING_MONEY;
            }
            p.corp_rating += rating;
        }
        let expired = self.world_time >= m.deadline;
        let m = &mut self.corp_state[k].missions[slot];
        m.result = Some((expired, rating));
        m.done = true;
        self.push_mission_text(k, slot);
    }
}
