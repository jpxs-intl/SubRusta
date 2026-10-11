use glam::Vec3;
use rosa_protocol::clientbound::game::ItemKind;

use crate::sim::Sim;

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
/// The randomize passes over the corporation list.
const SHUFFLES: i32 = 1024;
/// A disk left on the table rather than at a mission spot.
const NO_LOCATION: i32 = -1;

fn step(p: Vec3, r: Vec3, s: f32) -> Vec3 {
    r * s + p
}

impl Sim {
    /// corp_active_mission_count: an undecided mission still running counts 1 for an acquisition, 2 otherwise.
    fn corp_active_mission_count(&self, k: usize) -> i32 {
        self.corp_state[k].missions.iter().filter(|m| m.active && !m.done && self.world_time < m.deadline).map(|m| if m.kind <= 1 { 1 } else { 2 }).sum()
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
                let a = rand::random_range(0..n) as usize;
                let b = rand::random_range(0..n) as usize;
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
        } else if rand::random_ratio(3, 4) {
            return self.deal_transaction(list);
        } else if n == 3 {
            if rand::random_ratio(1, 4) {
                self.generate_twoteam_missions(&list[..3]);
                return 3;
            }
            true
        } else if rand::random::<bool>() || n <= 3 {
            let many = n > 2;
            if rand::random_ratio(1, 4) && many {
                return self.couple_or_single(list);
            }
            return self.chase_or_single(list, n);
        } else {
            if rand::random_ratio(1, 4) {
                return self.couple_or_single(list);
            }
            let size = if rand::random_ratio(1, 4) { 1 } else { 3 };
            self.create_mission_acquisition(&list[..size]);
            return size as i32;
        };
        if rand::random_ratio(1, 4) && couple_first { self.couple_or_single(list) } else { self.chase_or_single(list, n) }
    }

    /// A transaction for two, three or all of `list`, or a two-team deal for three.
    fn deal_transaction(&mut self, list: &[i32]) -> i32 {
        let n = list.len() as i32;
        let size = if !rand::random::<bool>() && n > 3 {
            if rand::random_ratio(1, 16) { 2 } else { 0 }
        } else if rand::random_ratio(1, 16) {
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
        if rand::random_ratio(3, 4) {
            self.generate_transaction_missions(&list[..3]);
        } else {
            self.generate_twoteam_missions(&list[..3]);
        }
        3
    }

    /// An acquisition for `size` corporations, or one in four times (with more than one to pick from) for the first.
    fn chase_or_single(&mut self, list: &[i32], size: i32) -> i32 {
        let size = if rand::random_ratio(3, 4) || list.len() <= 1 { size } else { 1 };
        self.create_mission_acquisition(&list[..size as usize]);
        size
    }

    /// An acquisition for the first two corporations, or one in four times the first alone.
    fn couple_or_single(&mut self, list: &[i32]) -> i32 {
        let size = if rand::random_ratio(3, 4) { 2 } else { 1 };
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
        let twice = rand::random_ratio(1, 4) as usize + 1;
        let funded = if twice < n { twice } else { 1 };
        let deadline = self.deadline(TRANSACTION_TIME);
        let colour = self.next_disk();
        let disk = ItemKind::try_from((ItemKind::DiskBlack as i32 + colour) as u8).unwrap();
        let location = if rand::random_ratio(3, 4) { rand::random_range(0..8) } else { NO_LOCATION };
        let values: Vec<i32> = (0..n)
            .map(|i| {
                if i == 0 {
                    return SELLER_VALUE;
                }

                let mut v = rand::random_range(0..5) * VALUE_STEP + BASE_VALUE;
                if funded < i {
                    v += rand::random_range(0..5) * VALUE_STEP;
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
                    let p = crate::sim::missions::MISSION_SPOTS[location as usize];
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
                let mut count = rand::random_range(0..3) + 2;
                if colour == GOLD {
                    count += rand::random_range(0..3) + 2;
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
                let a = rand::random_range(0..5) * VALUE_STEP;
                let b = rand::random_range(0..5) * VALUE_STEP;
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
                let mut count = rand::random_range(0..5) + 4;
                if colour == GOLD {
                    count += rand::random_range(0..3) + 2;
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
            let mut value = rand::random_range(0..5) * VALUE_STEP + BASE_VALUE;
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
            m.provided_cash = 0;
            m.location = location;
            m.rate = value as f32 * RATE_SCALE;
            self.push_mission_event(k, slot);
        }
        self.missions.counter += 1;
    }

    /// round_complete: each round corporation earns the round cash in its base and the value of the missions whose
    /// disks are there, less what its missions were given; its members share the earnings (larger teams split them
    /// further with bonusratio on) and its share price moves with them.
    pub(crate) fn round_complete(&mut self) {
        let divisors: [i32; ROUND_CORPS] = if self.modes.round.cfg.bonusratio != 0 {
            let counts: [i32; ROUND_CORPS] = std::array::from_fn(|k| {
                let team = rosa_protocol::Team::CORPORATIONS[k];
                self.players.iter().filter(|(_, p)| p.team == team).count() as i32
            });
            let least = counts.iter().fold(NO_TEAM_SIZE, |least, &n| if n < least && n > 0 { n } else { least });
            counts.map(|n| if n > 0 { n / least } else { 1 })
        } else {
            [1; ROUND_CORPS]
        };
        let weekly = self.modes.round.cfg.weekly;
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
        let e = crate::sim::economy::stock_event(&self.corporations, self.tick);
        self.events.push(e);
        for (_, p) in self.players.iter() {
            let e = p.make_update_round_event(self.tick);
            self.events.push(e);
        }
    }
}
