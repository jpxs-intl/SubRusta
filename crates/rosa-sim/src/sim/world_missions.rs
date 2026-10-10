use glam::Vec3;
use rosa_protocol::GameMode;

use super::Sim;
use crate::computer::fs::COMPUTER_CAPACITY;
use crate::world::building::{CORPORATION_BASE, LAB};

/// A lab's computers, where document missions are put (0x44da7000, 0x40c each): the building, how many missions it
/// has been given and its computers' volumes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MissionSpot {
    pub building: usize,
    pub used: i32,
    pub volumes: Vec<i32>,
}

/// The places a world mission's goods change hands (0x44da5cfc, 0x4c each).
pub const DROP_SPOTS: [(Vec3, &str); 8] = [
    (Vec3::new(1718.0, 46.0, 1262.0), "Hondo Park"),
    (Vec3::new(1778.0, 30.0, 1006.0), "Rio Granary"),
    (Vec3::new(1470.0, 50.0, 1494.0), "Isle of Burgers"),
    (Vec3::new(1282.0, 62.0, 1534.0), "Park above the Kamel Building"),
    (Vec3::new(1674.0, 66.0, 1510.0), "Bobson Dugnutt Museum"),
    (Vec3::new(1346.0, 38.0, 1298.0), "The Mall"),
    (Vec3::new(1606.0, 26.0, 1166.0), "Gas Station"),
    (Vec3::new(1546.0, 38.0, 1278.0), "Red Cube Park"),
];

/// The mission codenames (0x72cac0) run on into the project names (0x72cb00, 32 of them).
pub const NAMES: [&str; 40] = [
    "ATOMIC", "BRAZIL", "CROWN", "DENVER", "EAGLE", "FALCON", "GUILD", "HOUND", "ACTOR", "GOLD", "RIVER", "PIANO", "PILLOW", "ANIMAL", "HORSE", "HONEY", "BRANCH", "LAMP",
    "LION", "TENT", "LONDON", "NIGHT", "FISH", "FLOWER", "TRUCK", "GLASS", "GHOST", "KNIFE", "SPOON", "TRAFFIC", "VASE", "SUGAR", "KITCHEN", "SCOOTER", "KING", "QUEEN",
    "LIZARD", "CARAVAN", "DREAM", "DRESS",
];
const PROJECTS: usize = 8;
const NAME_FLAGS: usize = 512;
pub const MISSIONS: usize = 256;
const DAILY_MISSIONS: u32 = 7;
const PICK_TRIES: i32 = 0x400;
const DECOYS: i32 = 64;
const SHUFFLES: i32 = 64;
/// A mission's reward is 5000 to 15000 in steps of 2500, five times that for the day's boosted mission; a manager's
/// corporate rating past 1000 adds up to 1000%.
const REWARD_BASE: i32 = 5000;
const REWARD_STEP: i32 = 2500;
const BOOST: i32 = 5;
/// Each day step a corporation's open missions grow by its manager's bonus plus 10% a member, at most 100%.
const MEMBER_BONUS: i32 = 10;
const MAX_MEMBERS: i32 = 10;
/// The INTEL file of mission k is content k + 200.
pub const INTEL_CONTENT: i32 = 200;

/// A corporation's view of a world mission (0x86c each from mission +0x14): what it is worth to it, how often it has
/// asked for intel, its status lines and status word.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldMissionTeam {
    pub value: i32,
    pub status: i32,
    pub lines: Vec<String>,
    pub word: String,
    /// Whether the corporation has uploaded a file for it (mission +0x10 + team * 0x86c).
    pub uploaded: bool,
}

/// A world mission (world_missions, 0x21d74 each): a project whose document sits on a lab computer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldMission {
    pub active: bool,
    /// 0 untouched, 1 being traded, 2 done (+0x8).
    pub state: i32,
    pub reward: i32,
    pub teams: [WorldMissionTeam; 6],
    /// The content of its .DOC file (+0x1e874).
    pub document: i32,
    /// Its INTEL file's content (+0x21b60) and name (+0x21b64).
    pub intel: i32,
    pub filename: String,
    /// The document's name, the lab's streets and the project name (+0x21c70 on).
    pub names: [String; 4],
    pub high_value: bool,
}

/// What world mode keeps for its missions.
#[derive(Clone, Debug, Default)]
pub struct WorldMissions {
    pub spots: Vec<MissionSpot>,
    pub missions: Vec<WorldMission>,
    /// Which project names are taken today (something_world, cleared with the day's missions).
    pub names_used: Vec<bool>,
    /// The mission paying five times its reward (data_e3ba238) and the INTEL files made so far (data_e3ba23c).
    pub boosted: i32,
    pub intel_files: i32,
}

/// A name not yet taken today, tried at random up to 1024 times; `take` marks it taken.
fn pick_name(used: &mut Vec<bool>, take: bool) -> usize {
    if used.len() < NAME_FLAGS {
        used.resize(NAME_FLAGS, false);
    }
    let mut k = (crate::rng::rand() & 0x1f) as usize;
    let mut tries = PICK_TRIES;
    while used[k] {
        k = (crate::rng::rand() & 0x1f) as usize;
        tries -= 1;
        if tries == 0 {
            break;
        }
    }
    if take {
        used[k] = true;
    }
    k
}

/// One of the volumes, at random.
fn pick_volume(vols: &[i32]) -> i32 {
    if vols.is_empty() {
        return -1;
    }
    vols[(crate::rng::rand() % vols.len() as u32) as usize]
}

/// create_world_mode_mission_string: what the INTEL file says about a mission.
fn mission_string(file: &str, a: &str, b: &str, project: &str, high_value: bool) -> String {
    let mut s = String::new();
    match crate::rng::rand() % 3 {
        0 => s += "We have received reports that ",
        1 => s += "Reports indicate that ",
        _ => {}
    }
    s += &match crate::rng::rand() % 3 {
        0 => format!("the laboratory at {a} and {b} "),
        1 => format!("the lab at {a} and {b} "),
        _ => format!("{a} and {b} laboratory "),
    };
    s += match crate::rng::rand() % 3 {
        0 => "acquired a disk",
        1 => "has disks containing info",
        _ => "may have disks",
    };
    s += &format!(" the filename is {file} ");
    s += &if high_value { format!("project {project} is high value") } else { format!("project {project} is normal value") };
    s
}

impl Sim {
    /// init_mission_spots: in world mode each base computer gets a volume with MISSION.EXE and DECRYPT.EXE for its
    /// corporation, and each lab becomes a mission spot with a volume per computer.
    pub(crate) fn init_mission_spots(&mut self) {
        self.world_missions.spots.clear();
        // TODO: game type 0 (none) makes every lab a spot without volumes
        if self.gamemode != GameMode::World {
            return;
        }
        for c in &mut self.corp_state {
            c.computers.clear();
        }
        for i in 0..self.world.map.level.buildings.len() {
            let kind = self.world.map.level.buildings[i].kind;
            if kind == CORPORATION_BASE {
                let team = self.world.map.level.buildings[i].team;
                for m in 0..self.world.map.level.buildings[i].computers.len() {
                    let v = self.fs.alloc_volume(COMPUTER_CAPACITY);
                    self.world.map.level.buildings[i].computers[m].1 = v;
                    self.fs.add_file(v, 0, "MISSION.EXE", 0);
                    self.fs.add_file(v, 0, "DECRYPT.EXE", 0);
                    if let Some(c) = usize::try_from(team).ok().and_then(|t| self.corp_state.get_mut(t)) {
                        c.computers.push(v);
                    }
                }
            }
            if kind == LAB {
                let spot = self.world_missions.spots.len();
                self.world.map.level.buildings[i].spot = spot as i32;
                let mut volumes = Vec::new();
                for m in 0..self.world.map.level.buildings[i].computers.len() {
                    let v = self.fs.alloc_volume(COMPUTER_CAPACITY);
                    self.world.map.level.buildings[i].computers[m].1 = v;
                    volumes.push(v);
                }
                self.world_missions.spots.push(MissionSpot { building: i, used: 0, volumes });
            }
        }
    }

    /// The spawn_item_from_grid_cell part for computers: one placed in a building at one of its computer cells takes
    /// that computer's volume and, in a base, the corporation.
    pub(crate) fn place_set_computer(&mut self, id: usize, pos: Vec3, cell: glam::IVec3) {
        for b in &self.world.map.level.buildings {
            if !b.contains(pos, 0.0) {
                continue;
            }
            for &(p, v) in &b.computers {
                if p != cell {
                    continue;
                }
                if let Some(i) = self.items.get_mut(id) {
                    i.volume = v;
                }
                if b.kind == CORPORATION_BASE
                    && let Some(crate::sim::item_state::ItemState::Computer(c)) = self.items.get_mut(id).map(|i| &mut i.state)
                {
                    c.team = b.team;
                }
            }
        }
    }
}

impl Sim {
    pub fn mission_spots(&self) -> &[MissionSpot] {
        &self.world_missions.spots
    }

    pub fn corp_computers(&self, k: usize) -> &[i32] {
        &self.corp_state[k].computers
    }
}

impl Sim {
    /// The day's world missions (logic_world at MISSIONS_AT): the names freed, one of the 7 boosted, the missions made
    /// and their documents spread out.
    pub(crate) fn start_world_missions(&mut self) {
        self.world_missions.names_used = vec![false; NAME_FLAGS];
        self.world_missions.boosted = (crate::rng::rand() % DAILY_MISSIONS) as i32;
        for _ in 0..DAILY_MISSIONS {
            self.create_world_mission();
        }
        self.distribute_documents();
    }

    /// set_team_manager in world mode: the day's untouched missions worth something to the corporation are worth
    /// their reward plus the new manager's bonus.
    pub(crate) fn scale_missions_for_manager(&mut self, t: usize, rating: i32) {
        let bonus = ((rating - 1000) / 100).clamp(0, 1000);
        for m in self.world_missions.missions.iter_mut().take(DAILY_MISSIONS as usize) {
            let v = &mut m.teams[t].value;
            if m.state == 0 && *v > 0 {
                *v = bonus.wrapping_mul(m.reward) / 100 + m.reward;
            }
        }
    }

    /// The percentage a corporation's manager adds to its mission values: their corporate rating past 1000, a point
    /// per 100, up to 1000.
    fn manager_bonus(&self, t: usize) -> i32 {
        self.corp_state[t].manager.and_then(|p| self.players.get(p.idx())).map_or(0, |p| ((p.corp_rating - 1000) / 100).clamp(0, 1000))
    }

    /// increment_manager_rating: each of the day's untouched missions worth something to the corporation grows by
    /// the manager's bonus and 10% a member (100% from ten members).
    pub(crate) fn increment_manager_rating(&mut self, t: usize) {
        let bonus = self.manager_bonus(t);
        let members = self.corp_state[t].player_count;
        let rate = bonus + if members < MAX_MEMBERS { members * MEMBER_BONUS } else { MAX_MEMBERS * MEMBER_BONUS };
        for m in self.world_missions.missions.iter_mut().take(DAILY_MISSIONS as usize) {
            let v = &mut m.teams[t].value;
            if m.state == 0 && *v > 0 {
                *v = m.reward.wrapping_mul(rate) / 100 + m.reward;
            }
        }
    }

    pub(crate) fn street_name(&self, s: i32) -> String {
        usize::try_from(s).ok().and_then(|s| self.world.map.streets.streets.get(s)).map_or(String::new(), |s| s.name.to_string())
    }

    /// create_world_mode_mission: a project at the least used lab: its document on one of the lab's computers, a
    /// reward for three corporations (raised by their manager's corporate rating) and its INTEL file.
    fn create_world_mission(&mut self) {
        let w = &mut self.world_missions;
        if w.missions.len() < MISSIONS {
            w.missions.resize(MISSIONS, WorldMission::default());
        }
        let Some(idx) = w.missions.iter().position(|m| !m.active) else { return };
        w.missions[idx] = WorldMission { active: true, ..Default::default() };
        let n = w.spots.len();
        if n == 0 {
            return;
        }
        let least = w.spots.iter().map(|s| s.used).fold(0x10000, i32::min);
        let mut r = (crate::rng::rand() % n as u32) as usize;
        let mut tries = PICK_TRIES;
        while w.spots[r].used > least {
            r = (crate::rng::rand() % n as u32) as usize;
            tries -= 1;
            if tries == 0 {
                break;
            }
        }
        w.spots[r].used += 1;
        let building = w.spots[r].building;
        let Some(content) = self.fs.contents.iter().position(|c| !c.active) else { return };
        self.fs.contents[content].active = true;
        self.fs.contents[content].encrypted = false;
        self.world_missions.missions[idx].document = content as i32;
        let vol = pick_volume(&self.world_missions.spots[r].volumes);
        let k = pick_name(&mut self.world_missions.names_used, true);
        let doc = NAMES[PROJECTS + k];
        self.fs.add_file(vol, 0, &format!("{doc}.DOC"), content as i32);
        let high_value = crate::rng::rand() % 5 == 0;
        let mut reward = (crate::rng::rand() % 5) as i32 * REWARD_STEP + REWARD_BASE;
        if self.world_missions.boosted == idx as i32 {
            reward *= BOOST;
        }
        self.world_missions.missions[idx].reward = reward;
        for _ in 0..3 {
            let t = (crate::rng::rand() % 6) as usize;
            self.world_missions.missions[idx].teams[t].value = self.manager_bonus(t) * reward / 100 + reward;
        }
        let (a, b) = self.world.map.level.buildings.get(building).map_or((0, 0), |b| b.streets);
        let (a, b) = (self.street_name(a), self.street_name(b));
        let intel = idx as i32 + INTEL_CONTENT;
        let text = mission_string(doc, &a, &b, NAMES.get(idx).copied().unwrap_or(""), high_value);
        if let Some(c) = self.fs.content_mut(intel) {
            c.text = text.into_bytes();
        }
        self.fs.encipher(intel);
        let m = &mut self.world_missions.missions[idx];
        m.intel = intel;
        m.filename = format!("INTEL{idx:02}.CDE");
        m.names = [doc.to_string(), a, b, doc.to_string()];
        m.high_value = high_value;
        for t in &mut m.teams {
            t.word = "Open".into();
        }
    }

    /// world_distribute_document_missions: 64 decoy documents across each lab's computers, then each computer's files
    /// shuffled.
    fn distribute_documents(&mut self) {
        for s in 0..self.world_missions.spots.len() {
            for _ in 0..DECOYS {
                let vol = pick_volume(&self.world_missions.spots[s].volumes);
                let k = pick_name(&mut self.world_missions.names_used, false);
                let name = format!("{}.DOC", NAMES[PROJECTS + k]);
                if self.fs.find_file(vol, 0, &name) == -1 {
                    let c = self.fs.alloc_content();
                    self.fs.add_file(vol, 0, &name, c);
                }
            }
        }
        for s in 0..self.world_missions.spots.len() {
            for v in self.world_missions.spots[s].volumes.clone() {
                let n = self.fs.volume(v).map_or(0, |x| x.files.len());
                if n <= 1 {
                    continue;
                }
                for _ in 0..SHUFFLES {
                    let a = (crate::rng::rand() % n as u32) as usize;
                    let b = (crate::rng::rand() % n as u32) as usize;
                    if a != b
                        && let Some(vol) = self.fs.volume_mut(v)
                    {
                        vol.files.swap(a, b);
                    }
                }
            }
        }
    }

    pub fn world_mission_list(&self) -> &[WorldMission] {
        &self.world_missions.missions
    }
}

impl Sim {
    pub fn run_start_world_missions(&mut self) {
        self.start_world_missions();
    }

    pub fn world_boosted(&self) -> i32 {
        self.world_missions.boosted
    }
}

impl Sim {
    /// mission_update_file_trade_state: a corporation uploaded the mission's document. If the mission is worth
    /// something to it, it is paid and the mission closes for everyone else; otherwise it holds the file for trade
    /// and each corporation the mission is worth something to is told, by status line and memo.
    pub(crate) fn file_trade(&mut self, m: usize, team: usize) {
        use super::corporations::TEAM_NAMES;
        self.world_missions.missions[m].teams[team].uploaded = true;
        if self.world_missions.missions[m].state > 1 {
            return;
        }
        let value = self.world_missions.missions[m].teams[team].value;
        if value <= 0 {
            {
                let t = &mut self.world_missions.missions[m].teams[team];
                t.lines = vec!["TRADE".into()];
                t.word = "Trade".into();
            }
            for i in 0..6 {
                let mission = &mut self.world_missions.missions[m];
                if i == team || mission.teams[i].value <= 0 {
                    continue;
                }
                let state = mission.state;
                let v = mission.teams[i].value;
                let ti = &mut mission.teams[i];
                if state == 0 {
                    ti.lines.clear();
                }
                ti.lines.push(format!("{} HAS THE FILE  VALUE: ${}", TEAM_NAMES[team], v));
                ti.word = "Trade".into();
                let memo = format!("{} has obtained the file for the
{} project", TEAM_NAMES[team], mission.names[3]);
                mission.teams[team].lines.push(format!("{} WANTS THE FILE", TEAM_NAMES[i]));
                self.place_corporation_memo(i, memo.as_bytes());
            }
            self.world_missions.missions[m].state = 1;
            return;
        }
        self.corp_state[team].money += value;
        let mission = &mut self.world_missions.missions[m];
        for (i, t) in mission.teams.iter_mut().enumerate() {
            if i == team {
                t.lines = vec!["MISSION SUCCESS".into(), format!("VALUE: ${value}")];
                t.word = "Success".into();
            } else {
                t.lines = vec!["MISSION CLOSED".into()];
                t.word = "Closed".into();
            }
        }
        mission.state = 2;
        if self.corp_state[team].intel > 0 {
            self.corp_state[team].intel = 1;
        }
    }
}

impl Sim {
    pub fn run_generate_team_mission_memo(&mut self, t: usize) {
        self.generate_team_mission_memo(t);
    }

    pub fn world_intel_files(&self) -> i32 {
        self.world_missions.intel_files
    }

    pub fn corp_money_intel(&self, k: usize) -> (i32, i32) {
        (self.corp_state[k].money, self.corp_state[k].intel)
    }

    pub fn set_corp_intel(&mut self, k: usize, v: i32) {
        self.corp_state[k].intel = v;
    }

    pub fn set_computer_team(&mut self, id: usize, team: i32) {
        if let Some(crate::sim::item_state::ItemState::Computer(c)) = self.items.get_mut(id).map(|i| &mut i.state) {
            c.team = team;
        }
    }
}

/// A corporation's intel once its memo is faxed (corporation +0xc).
const INTEL_FAXED: i32 = 11;
/// Corporations give up looking for a mission worth something to one of them after 100 tries.
const MEMO_TRIES: i32 = 100;
/// A memo's three INTEL parts come in a shuffled order (64 swaps).
const PART_SHUFFLES: i32 = 64;
/// An intel disk's volume, and how far the intel car must drive.
const DISK_CAPACITY: i32 = 360;
const INTEL_CAR_DISTANCE: f32 = 768.0;
/// The intel car's mission expires an hour of the world clock on.
const INTEL_TIME: i32 = 216000;
/// The intel car's driver holds the key in slot 4 and the disk in slot 3.
const INTEL_KEY_SLOT: usize = 4;
const INTEL_DISK_SLOT: usize = 3;
/// The intel car's key and disk are made here before being handed to the driver.
const INTEL_ITEMS_POS: Vec3 = Vec3::new(62.0, 10.0, 60.0);

fn rand_mod(n: i32) -> i32 {
    crate::rng::rand() as i32 % n
}

/// The INTEL part naming the lab's streets.
fn streets_part(a: &str, b: &str) -> String {
    match rand_mod(3) {
        0 => format!("the laboratory at {a} and {b} has the data. "),
        1 => format!("the lab at {a} and {b} has made great strides. "),
        _ => format!("we are increasing development at the {a} and {b} laboratory. "),
    }
}

/// The INTEL part naming the project.
fn project_part(project: &str) -> String {
    match rand_mod(3) {
        0 => format!("project {project} has made progress in many areas. "),
        1 => format!("there are new developments in the {project} project. "),
        _ => format!("the {project} project will revolutionize the industry. "),
    }
}

/// The INTEL part about the project's worth: office chatter, or for the boosted mission a hint.
fn worth_part(boosted: bool) -> &'static str {
    if boosted {
        return if rand_mod(2) == 1 { "this is a high priority project. " } else { "this project is extremely high value. " };
    }
    match rand_mod(4) {
        1 => "the company picnic is next saturday. ",
        2 => "next friday is hawaiian shirt day. ",
        3 => "we need to create more synergy. ",
        _ => "we are now adding coversheets to tps reports. ",
    }
}

impl Sim {
    /// generate_team_mission_memo: the corporation is told about one of the day's missions it has asked least about
    /// (among those not yet traded) that some corporation with players wants. An encrypted INTEL file describes it,
    /// and the file is either left on a disk in a lab's lobby or carried by a town car, which the memo tells of.
    pub(crate) fn generate_team_mission_memo(&mut self, t: usize) {
        let daily = DAILY_MISSIONS as usize;
        let missions = &self.world_missions.missions;
        if missions.len() < daily {
            return;
        }
        let least_asked = missions[..daily].iter().map(|m| m.teams[t].status).fold(256, i32::min);
        let least_state = missions[..daily].iter().map(|m| m.state).fold(256, i32::min);
        if least_state > 0 {
            return;
        }
        let list: Vec<usize> = (0..daily).filter(|&i| missions[i].teams[t].status == least_asked && missions[i].state == least_state).collect();
        if list.is_empty() {
            return;
        }
        let mut m = 0;
        for tries in 1.. {
            m = list[rand_mod(list.len() as i32) as usize];
            let wanted = (0..6).any(|j| self.corp_state[j].player_count > 0 && self.world_missions.missions[m].teams[j].value > 0);
            if wanted || tries >= MEMO_TRIES {
                break;
            }
        }
        let mut order = [0, 1, 2];
        for _ in 0..PART_SHUFFLES {
            let a = rand_mod(3) as usize;
            let b = rand_mod(3) as usize;
            order.swap(a, b);
        }
        self.world_missions.missions[m].teams[t].status += 1;
        self.corp_state[t].intel = INTEL_FAXED;
        let boosted = self.world_missions.boosted == m as i32;
        let names = self.world_missions.missions[m].names.clone();
        let mut text = String::new();
        for part in order {
            text += &match part {
                0 => streets_part(&names[1], &names[2]),
                1 => project_part(&names[3]),
                _ => worth_part(boosted).to_string(),
            };
        }
        let content = self.fs.alloc_content();
        if let Some(c) = self.fs.content_mut(content) {
            c.text = text.into_bytes();
        }
        self.fs.encipher(content);
        self.world_missions.intel_files += 1;
        let file = format!("INTEL{:02}.CDE", self.world_missions.intel_files);
        if crate::rng::rand() & 1 != 0 {
            return self.intel_disk_memo(t, &file, content);
        }
        let expiry = self.world_time + INTEL_TIME;
        let Some(car) = self.spawn_mission_intel_car(&file, content, expiry) else { return };
        let (hour, minute, pm) = super::memos::clock_time(expiry);
        let route: Vec<i32> = {
            let c = &self.traffic.cars[car];
            c.route[..(c.route_len.max(0) as usize).min(c.route.len())].iter().map(|s| s.street).collect()
        };
        let (Some(&first), Some(&last)) = (route.first(), route.last()) else { return };
        let mut memo = String::from("There is in a town car driving around the city with\nvaluable information.\n");
        if let Some((a, b)) = self.route_corner(first) {
            memo += &format!("The car will be driving from {a} and {b}\n");
        }
        if let Some((a, b)) = self.route_corner(last) {
            memo += &format!("to {a} and {b}.\n");
        }
        memo += &format!("This mission expires at {hour}:{minute:02} {}.\n", if pm { "PM" } else { "AM" });
        self.place_corporation_memo(t, memo.as_bytes());
    }

    /// The two streets crossing at a route street's first intersection (east or else west, south or else north).
    fn route_corner(&self, street: i32) -> Option<(String, String)> {
        let streets = &self.world.map.streets;
        let s = streets.streets.get(usize::try_from(street).ok()?)?;
        let i = streets.intersections.get(s.intersections[0])?;
        let a = if i.streets[0] != -1 { i.streets[0] } else { i.streets[2] };
        let b = if i.streets[1] != -1 { i.streets[1] } else { i.streets[3] };
        (a != -1 && b != -1).then(|| (self.street_name(a), self.street_name(b)))
    }

    /// The intel left on a blue disk in a random lab's lobby, and the memo saying where.
    fn intel_disk_memo(&mut self, t: usize, file: &str, content: i32) {
        let spots = self.world_missions.spots.len() as i32;
        if spots == 0 {
            return;
        }
        let building = self.world_missions.spots[rand_mod(spots) as usize].building;
        let (lobby, (a, b)) = self.world.map.level.buildings.get(building).map_or((Vec3::ZERO, (-1, -1)), |b| (b.lobby, b.streets));
        if let Some(disk) = self.create_item(rosa_protocol::clientbound::game::ItemKind::DiskBlue, lobby, None, rosa_physics::rotation::IDENTITY) {
            let vol = self.fs.alloc_volume(DISK_CAPACITY);
            self.items.get_mut(disk).unwrap().volume = vol;
            self.fs.add_file(vol, 0, file, content);
        }
        let memo = format!("We have intercepted communications\nThe disk with the info in the lobby of the lab at\n{} and {}\nFilename: {file}", self.street_name(a), self.street_name(b));
        self.place_corporation_memo(t, memo.as_bytes());
    }

    /// spawn_mission_intel_car: a town car driving to a street at least 768 away, its driver holding the key and a
    /// red disk with the intel, and its guards; all of them leave at `expiry`. The traffic car, unless there are no
    /// streets.
    pub(crate) fn spawn_mission_intel_car(&mut self, file: &str, content: i32, expiry: i32) -> Option<usize> {
        use rosa_physics::rotation::IDENTITY;
        use rosa_protocol::{Team, clientbound::game::ItemKind};
        if self.world.map.streets.streets.is_empty() {
            return None;
        }
        let (car, vid) = self.spawn_chase_car(Some(INTEL_CAR_DISTANCE));
        let Some(vid) = vid else { return Some(car) };
        let v = self.vehicles.get(vid).unwrap();
        let (vpos, vrot) = (v.pos, v.rot);
        let Some((driver, driver_human)) = self.create_mission_bot(expiry, None, vpos, vrot, vid, 0) else { return Some(car) };
        if let Some(key) = self.create_item(ItemKind::Key, INTEL_ITEMS_POS, None, IDENTITY) {
            self.items.get_mut(key).unwrap().state = super::item_state::ItemState::Key { vehicle: Some(vid) };
            if let Some(h) = driver_human {
                self.link_to_slot(h, key, INTEL_KEY_SLOT);
            }
        }
        if let Some(disk) = self.create_item(ItemKind::DiskRed, INTEL_ITEMS_POS, None, IDENTITY) {
            let vol = self.fs.alloc_volume(DISK_CAPACITY);
            self.items.get_mut(disk).unwrap().volume = vol;
            self.fs.add_file(vol, 0, file, content);
            if let Some(h) = driver_human {
                self.link_to_slot(h, disk, INTEL_DISK_SLOT);
            }
        }
        if let Some(p) = self.players.get_mut(driver.idx()) {
            p.team = Team::Mission;
            p.bot.unk_2d24 = 1;
            p.bot.waypoint = 0;
            p.bot.waypoint_count = 0;
        }
        self.create_guards(expiry, None, vpos, vrot, vid);
        Some(car)
    }
}
