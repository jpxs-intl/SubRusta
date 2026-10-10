use glam::Vec3;
use rosa_physics::rotation::{IDENTITY, rotate_orientation};
use rosa_protocol::{
    GameMode, Team,
    clientbound::game::{
        GameState, ItemKind, MenuButton, MenuType,
        events::{Event, ServerEvent, chat::ChatType, team_door::EventTeamDoor},
    },
};

use super::{Sim, item_state::{BILL_VALUES, Cash, ItemState, PhoneStatus}};
use crate::{
    PlayerId,
    world::{area::MESH, city_objects::GARAGE_DOOR, trace::line_intersect_level},
};

/// A corporation takes up to 32 applications at once.
const MAX_APPLICANTS: usize = 32;
/// The team phones: the manager's at the table is numbered (team + 1) * 1111, and the one after it goes with it.
const PHONE_NUMBER_STEP: i32 = 1111;
/// A member can work the garage door from up to 4 outside the base.
const DOOR_REACH: f32 = 4.0;
/// A replacement phone costs 100; everything else costs its item type's price.
const PHONE_PRICE: i32 = 100;
/// A corporation member can requisition 10 items.
/// A requisitioned disk's volume.
const REQUISITION_DISK_CAPACITY: i32 = 360;
const REQUISITION_LIMIT: i32 = 9;

/// The tab buttons carry this flag with the tab in the low byte; a button of the open tab is greyed out as -1.
const TAB: i32 = 0x1000000;
const TAB_HIRING: i32 = TAB | 1;
const TAB_FIRING: i32 = TAB | 2;
const TAB_TEAM: i32 = TAB | 3;
const TAB_REQUISITION: i32 = TAB | 4;
const HIRE: i32 = 3;
const FIRE: i32 = 6;

const APPLY_MANAGER: i32 = 0;
const APPLY: i32 = 1;
const QUIT_TEAM: i32 = 0;
const DEPOSIT: i32 = 2;
const WITHDRAW_20: i32 = 3;
const WITHDRAW_100: i32 = 4;
const WITHDRAW_1000: i32 = 5;
const REPLACE_PHONE: i32 = 12;
/// The requisition buttons and the item each buys.
const REQUISITIONS: [(i32, &str, ItemKind); 8] = [
    (0, "Buy Disk", ItemKind::DiskBlack),
    (1, "Buy Briefcase", ItemKind::Briefcase),
    (6, "Buy Walkie-Talkie", ItemKind::Radio),
    (7, "Buy 9mm", ItemKind::Pistol),
    (8, "Buy 9mm Magazine", ItemKind::PistolMag),
    (9, "Buy MP5", ItemKind::Mp5),
    (10, "Buy MP5 Magazine", ItemKind::Mp5Mag),
    (11, "Buy Bandage", ItemKind::Bandage),
];

/// A corporation's people and account (game_mode_state.corporations +0x324 manager, +0x5d80 applicants; world state
/// 0x10591640 + team * 0x14 money and credit).
#[derive(Clone, Debug, Default)]
pub struct CorpState {
    pub manager: Option<PlayerId>,
    pub applicants: Vec<PlayerId>,
    pub money: i32,
    /// How far the account can go below zero: the manager's corporate rating when they took over (world mode).
    pub credit: i32,
    /// The garage door (+0x320).
    pub door_open: bool,
    /// Whether the door cells are open: they follow the door events (server_main handling event 0xa), which round mode
    /// also sends without changing the flag.
    pub door_cells_open: bool,
    /// The corporation's round funds (+0x50).
    pub funds: i32,
    /// What setting up this round's missions took from the corporation (+0x4c, negative).
    pub spent: i32,
    pub missions: [super::missions::Mission; super::missions::MISSION_SLOTS],
    /// Active players on the team (+0x54), counted every tick.
    pub player_count: i32,
    /// Which of the base's car spaces hold a vehicle (+0x460 in each 0x34 byte space).
    pub car_space_taken: Vec<bool>,
    /// The accounts the round manager fired (+0xccc count, up to 128).
    pub fired: Vec<u32>,
    /// The intel the corporation can ask for (money record +0xc: 1 to 8 can request, 0 requested, 11 faxed, others
    /// none) and how many screens it has printed (+0x10, 4 at most).
    pub intel: i32,
    pub prints: i32,
    /// The volumes of the base's computers (+0x320 count, from game_mode_state + 0x320 + k * 0x5bc4).
    pub computers: Vec<i32>,
}

fn button(id: i32, text: &str) -> MenuButton {
    MenuButton { id, text: text.to_string(), extra: 0 }
}

fn menu_type(id: u8) -> MenuType {
    match id {
        20 => MenuType::WorldCorpApplication,
        21 => MenuType::WorldCorpTabs,
        22 => MenuType::WorldCorpHiring,
        23 => MenuType::WorldCorpFiring,
        24 => MenuType::WorldCorpTeam,
        25 => MenuType::WorldCorpRequistion,
        _ => MenuType::Empty,
    }
}

fn team_index(team: Team) -> Option<usize> {
    Team::CORPORATIONS.iter().position(|&t| t == team)
}

impl Sim {
    /// The world mode corporation part of logic_player, for a player whose human is in no shop or bank: inside a base
    /// a player without a team can apply to join (or to manage it when it has no manager), and a member gets its
    /// requisition page.
    pub(crate) fn corp_base_menu(&mut self, pid: PlayerId, pos: Vec3) {
        let Some(k) = self.world.map.level.bases.iter().position(|b| b.contains(pos)) else {
            if let Some(p) = self.players.get_mut(pid.idx()) {
                p.menu = MenuType::Empty;
            }
            return;
        };
        let manager = self.corp_state[k].manager;
        let applied = self.corp_state[k].applicants.contains(&pid);
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        let member = team_index(p.team) == Some(k);
        if p.team == Team::Spectator {
            p.menu = MenuType::WorldCorpApplication;
            match manager {
                None => p.push_button(APPLY_MANAGER, "Apply Manager", None),
                Some(_) if !applied => {
                    p.push_button(APPLY, "Apply", None);
                    self.remove_player_from_corps(pid);
                }
                Some(_) => {}
            }
            return;
        }
        match manager {
            Some(_) if member && matches!(p.menu, MenuType::WorldCorpApplication | MenuType::Empty) => p.menu = MenuType::WorldCorpRequistion,
            Some(_) => {}
            None => {
                if member {
                    p.menu = MenuType::WorldCorpApplication;
                }
                p.push_button(APPLY_MANAGER, "Apply Manager", None);
            }
        }
    }

    /// The tab part of logic_player in world mode: a corporation member with menus 21 to 26 open gets the tabs (the
    /// manager also hiring and firing) and the open tab's buttons.
    pub(crate) fn corp_tab_menu(&mut self, pid: PlayerId) {
        let Some(p) = self.players.get(pid.idx()) else { return };
        let Some(k) = team_index(p.team) else { return };
        let menu = p.menu as u8;
        if !(21..=26).contains(&menu) {
            return;
        }
        let tab = |id: i32, open: u8| if menu == open { -1 } else { id };
        let managing = self.corp_state[k].manager == Some(pid);
        let mut buttons = vec![button(tab(TAB_REQUISITION, 25), "Requisition")];
        if managing {
            buttons.push(button(tab(TAB_HIRING, 22), "Hiring"));
            buttons.push(button(tab(TAB_FIRING, 23), "Firing"));
            buttons.push(button(tab(TAB_TEAM, 24), "Team"));
            if menu == 22 {
                for &a in &self.corp_state[k].applicants {
                    let name = self.players.get(a.idx()).map_or("", |p| p.username.as_str());
                    buttons.push(button(((a.0 as i32) << 16) + HIRE, &format!("Hire {name}")));
                }
            }
            if menu == 23 {
                for (i, other) in self.players.iter().filter(|&(i, o)| i != pid.idx() && o.team == p.team) {
                    buttons.push(button(((i as i32) << 16) + FIRE, &format!("Fire {}", other.username)));
                }
            }
        } else {
            buttons.push(button(tab(TAB_TEAM, 24), "Team"));
        }
        if menu == 25 {
            for (id, text, _) in REQUISITIONS {
                buttons.push(button(id, text));
            }
            buttons.push(button(DEPOSIT, "Deposit"));
            if managing {
                buttons.push(button(WITHDRAW_20, "Withdrawl $20"));
                buttons.push(button(WITHDRAW_100, "Withdrawl $100"));
                buttons.push(button(WITHDRAW_1000, "Withdrawl $1000"));
                buttons.push(button(REPLACE_PHONE, "Replace Phone"));
            }
        }
        if menu == 24 {
            buttons.push(button(QUIT_TEAM, "Quit Team"));
        }
        if let Some(p) = self.players.get_mut(pid.idx()) {
            p.menu_buttons.clear();
            for b in buttons {
                p.push_button(b.id, &b.text, None);
            }
        }
    }

    /// The corporation money shown to each player (player_simulation): their corporation's money and credit.
    pub(crate) fn copy_corp_money(&mut self) {
        for (_, p) in self.players.iter_mut() {
            let c = team_index(p.team).map(|k| &self.corp_state[k]);
            p.corp_money = c.map_or(0, |c| c.money);
            p.corp_credit = c.map_or(0, |c| c.credit);
        }
    }

    /// corp_manager_menu_action: the buttons of menus 20 to 26 in a corporation base (the last one the human is in).
    pub(crate) fn corp_menu_action(&mut self, pid: PlayerId, action: u8, b: i32) {
        let Some(p) = self.players.get(pid.idx()) else { return };
        let Some(pos) = p.human.and_then(|h| self.humans.get(h)).map(|h| h.pos) else { return };
        let Some(k) = self.world.map.level.bases.iter().rposition(|base| base.contains(pos)) else { return };
        let home = team_index(p.team) == Some(k);
        if (21..=26).contains(&action) {
            if b & TAB != 0 {
                let menu = match b & 255 {
                    0 => 21,
                    1 => 22,
                    2 => 23,
                    3 => 24,
                    4 => 25,
                    _ => return,
                };
                if let Some(p) = self.players.get_mut(pid.idx()) {
                    p.menu = menu_type(menu);
                }
                return;
            }
            if action == 24 && b == QUIT_TEAM && home {
                self.set_player_team(pid, Team::Spectator);
                self.update_player_event(pid);
                return;
            }
        }
        if !home {
            if action == 20 {
                self.corp_apply(pid, k, b);
            }
            return;
        }
        let manager = self.corp_state[k].manager;
        if manager == Some(pid) {
            match action {
                22 if b & 255 == HIRE => self.corp_hire(k, b >> 16),
                22 => {}
                23 => self.corp_fire(pid, k, b >> 16),
                25 => self.corp_requisition(pid, k, b),
                20 => self.corp_apply(pid, k, b),
                _ => {}
            }
            return;
        }
        match action {
            25 if manager.is_some() => self.corp_requisition(pid, k, b),
            20 => self.corp_apply(pid, k, b),
            _ => {}
        }
    }

    /// Applying in a base: "Apply Manager" makes the player its manager when it has none, "Apply" puts them on its
    /// list for the manager to hire.
    fn corp_apply(&mut self, pid: PlayerId, k: usize, b: i32) {
        match b {
            APPLY_MANAGER if self.corp_state[k].manager.is_none() => {
                self.remove_player_from_corps(pid);
                self.set_player_team(pid, Team::CORPORATIONS[k]);
                if self.corp_state[k].manager.is_none() {
                    self.set_team_manager(k, pid);
                }
                if let Some(p) = self.players.get_mut(pid.idx()) {
                    p.menu = MenuType::WorldCorpRequistion;
                }
                self.update_player_event(pid);
                self.broadcast_corp_missions(k);
                self.corp_state[k].applicants.clear();
            }
            APPLY if self.corp_state[k].manager.is_some() => {
                let list = &mut self.corp_state[k].applicants;
                if !list.contains(&pid) && list.len() < MAX_APPLICANTS {
                    list.push(pid);
                }
            }
            _ => {}
        }
    }

    fn corp_hire(&mut self, k: usize, target: i32) {
        let target = PlayerId(target as u32);
        if self.players.get(target.idx()).is_none() {
            return;
        }
        self.set_player_team(target, Team::CORPORATIONS[k]);
        self.update_player_event(target);
        self.remove_player_from_corps(target);
        self.broadcast_corp_missions(k);
    }

    fn corp_fire(&mut self, pid: PlayerId, k: usize, target: i32) {
        let target = PlayerId(target as u32);
        if target == pid || self.players.get(target.idx()).is_none_or(|p| team_index(p.team) != Some(k)) {
            return;
        }
        self.set_player_team(target, Team::Spectator);
        self.update_player_event(target);
    }

    /// The requisition page: items paid from the corporation's money, depositing the cash in the right hand, and for
    /// the manager withdrawing bills and replacing the team phone.
    fn corp_requisition(&mut self, pid: PlayerId, k: usize, b: i32) {
        let Some(p) = self.players.get(pid.idx()) else { return };
        let Some(human) = p.human else { return };
        let kind = match b {
            REPLACE_PHONE => Some(ItemKind::Phone),
            _ => REQUISITIONS.iter().find(|r| r.0 == b).map(|r| r.2),
        };
        if let Some(kind) = kind {
            if p.items_bought > REQUISITION_LIMIT {
                return;
            }
            if kind == ItemKind::Phone {
                self.disconnect_phone(k);
            }
            let Some(item) = self.give_item(human, kind as u8, 0..7, |state| match state {
                ItemState::Radio { channel, .. } => *channel = k as i32,
                ItemState::Phone(phone) => {
                    phone.texture = 0;
                    phone.number = (k as i32 + 1) * PHONE_NUMBER_STEP;
                }
                _ => {}
            }) else {
                return;
            };
            if kind == ItemKind::DiskBlack {
                let vol = self.fs.alloc_volume(REQUISITION_DISK_CAPACITY);
                self.items.get_mut(item).unwrap().volume = vol;
            }
            if kind == ItemKind::Phone {
                self.phone_update(item);
            }
            let price = if kind == ItemKind::Phone { PHONE_PRICE } else { self.item_types[kind as usize].price };
            self.corp_state[k].money -= price;
            if let Some(p) = self.players.get_mut(pid.idx()) {
                p.items_bought += 1;
            }
            return;
        }
        match b {
            DEPOSIT => self.corp_deposit(human, k),
            WITHDRAW_20..=WITHDRAW_1000 if self.corp_state[k].manager == Some(pid) => self.corp_withdraw(human, k, b),
            _ => {}
        }
    }

    /// Deposit: the cash in the right hand goes into the corporation's account.
    fn corp_deposit(&mut self, human: usize, k: usize) {
        let Some(slot) = self.humans.get(human).map(|h| h.inventory[0]) else { return };
        if slot.count <= 0 {
            return;
        }
        let Some(item) = self.items.get_mut(slot.items[0] as usize) else { return };
        let Some(value) = item.state.cash().map(Cash::value) else { return };
        self.corp_state[k].money += value;

        self.mark_item_for_deletion(slot.items[0] as usize);
    }

    /// Withdrawl: a $20, $100 or $1000 bill onto the cash in hand (the right hand, or the left when the right holds
    /// something else), or into an empty hand as new cash, as long as the account stays within its credit.
    fn corp_withdraw(&mut self, human: usize, k: usize, b: i32) {
        let code = match b {
            WITHDRAW_100 => 5,
            WITHDRAW_1000 => 6,
            _ => 3,
        };
        let value = BILL_VALUES[code as usize];
        let c = &self.corp_state[k];
        if c.money - value < -c.credit {
            return;
        }
        let Some(inv) = self.humans.get(human).map(|h| h.inventory) else { return };
        let is_cash = |id: i32| self.items.get(id as usize).is_some_and(|i| i.item_type == ItemKind::CashWorld);
        let slot = if inv[0].count > 0 && !is_cash(inv[0].items[0]) { 1 } else { 0 };
        if inv[slot].count <= 0 {
            self.give_item(human, ItemKind::CashWorld as u8, slot..slot + 1, |state| {
                if let ItemState::Cash(cash) = state {
                    cash.bills = 0;
                    cash.codes = code;
                }
            });
        } else {
            let Some(cash) = self.items.get_mut(inv[slot].items[0] as usize).and_then(|i| i.state.cash_mut()) else { return };
            if !cash.insert(0, code) {
                return;
            }
        }
        self.corp_state[k].money -= value;
    }

    /// remove_player_form_corp: takes the player off every corporation's list of applicants.
    /// mission_broadcast_text_lines: each of the corporation's active missions is announced again.
    fn broadcast_corp_missions(&mut self, k: usize) {
        for slot in 0..super::missions::MISSION_SLOTS {
            if self.corp_state[k].missions[slot].active {
                self.push_mission_event(k, slot);
            }
        }
    }

    pub(crate) fn remove_player_from_corps(&mut self, pid: PlayerId) {
        for c in &mut self.corp_state {
            if let Some(i) = c.applicants.iter().position(|&a| a == pid) {
                c.applicants.swap_remove(i);
            }
        }
    }

    /// set_team_manager: the manager gets the team phone on the table and, in world mode, their corporate rating as the
    /// account's credit.
    pub(crate) fn set_team_manager(&mut self, k: usize, pid: PlayerId) {
        if self.gamemode == GameMode::World {
            let rating = self.players.get(pid.idx()).map_or(0, |p| p.corp_rating);
            self.corp_state[k].credit = rating.max(0);
            self.scale_missions_for_manager(k, rating);
        }
        self.disconnect_phone(k);
        let base = &self.world.map.level.bases[k];
        let mut rot = IDENTITY;
        let axis = rot[1];
        rotate_orientation(&mut rot, axis, (base.table_orientation as f64 - 90.0_f64.to_radians()) as f32);
        if let Some(id) = self.create_item(ItemKind::Phone, base.table, None, rot) {
            if let Some(phone) = self.items.get_mut(id).and_then(|i| i.state.phone_mut()) {
                phone.texture = 0;
                phone.number = (k as i32 + 1) * PHONE_NUMBER_STEP;
            }
            self.phone_update(id);
        }
        self.corp_state[k].manager = Some(pid);
        let name = self.players.get(pid.idx()).map_or(String::new(), |p| p.username.clone());
        self.send_chat(&format!("{name} is now manager of {}", Team::CORPORATION_NAMES[k]), ChatType::Announce, -1, 0);
    }

    /// disconnect_phone: the team's two phones hang up on whoever they were talking to and despawn.
    fn disconnect_phone(&mut self, k: usize) {
        let number = (k as i32 + 1) * PHONE_NUMBER_STEP;

        for id in self.items.ids() {
            let Some(item) = self.items.get(id).filter(|i| i.item_type == ItemKind::Phone) else { continue };
            let Some(phone) = item.state.phone().filter(|p| p.number == number || p.number == number + 1) else { continue };

            if let Some(other) = phone.connected {
                if let Some(o) = self.items.get_mut(other).and_then(|i| i.state.phone_mut()) {
                    o.status = PhoneStatus::Idle;
                    o.connected = None;
                    o.display_number = 0;
                }

                self.phone_update(other);
            }

            let Some(item) = self.items.get_mut(id) else { continue };
            if let Some(phone) = item.state.phone_mut() {
                phone.status = PhoneStatus::Idle;
                phone.connected = None;
            }

            self.mark_item_for_deletion(id);
        }
    }

    /// update_corp_player_ratings: a tenth of the corporation's money (doubled when in debt) goes to its members'
    /// corporate ratings, half to the manager and half split among the rest (all of it when one member), and the
    /// account is emptied.
    pub(crate) fn update_corp_player_ratings(&mut self, k: usize) {
        let team = Team::CORPORATIONS[k];
        let members = self.players.iter().filter(|(_, p)| p.team == team).count() as i32;
        if members == 0 {
            return;
        }
        let tenth = self.corp_state[k].money / 10;
        let share = if tenth < 0 { tenth * 2 } else { tenth };
        let (manager_share, member_share) = if members == 1 { (share, share) } else { (share / 2, share / 2 / (members - 1)) };
        let manager = self.corp_state[k].manager;
        for (i, p) in self.players.iter_mut().filter(|(_, p)| p.team == team) {
            p.corp_rating += if manager.is_some_and(|m| m.idx() == i) { manager_share } else { member_share };
        }
        self.corp_state[k].money = 0;
    }

    /// A player stops managing the corporation they leave (in world mode after paying out its money).
    pub(crate) fn leave_corp_management(&mut self, pid: PlayerId, team: Team) {
        let Some(k) = team_index(team) else { return };
        if self.corp_state[k].manager != Some(pid) {
            return;
        }
        if self.gamemode == GameMode::World {
            self.update_corp_player_ratings(k);
        }
        self.corp_state[k].manager = None;
    }

    /// Opens or closes a corporation's garage door (create_event_toggle_team_door); the door cells follow at the end of
    /// the tick (apply_team_doors).
    pub(crate) fn set_team_door(&mut self, k: usize, open: bool) {
        self.corp_state[k].door_open = open;
        self.push_team_door_event(k, open);
    }

    /// create_event_toggle_team_door: the event, and the door cells following it at the end of the tick.
    pub(crate) fn push_team_door_event(&mut self, k: usize, open: bool) {
        self.corp_state[k].door_cells_open = open;
        let e = EventTeamDoor { team_id: k as i32, door_open: open };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::TeamDoor(e) });
    }

    /// corporation_update_door_blocks for every base: an open door's cells are emptied, a closed door's hold the door.
    pub(crate) fn apply_team_doors(&mut self) {
        let level = &mut self.world.map.level;
        for (k, base) in level.bases.iter().enumerate() {
            let Some((cells, dir)) = base.door else { continue };
            let word = if self.corp_state[k].door_cells_open { 0 } else { dir << 10 | GARAGE_DOOR };
            for c in cells {
                level.area.set_object(c.x, c.y, c.z, word);
            }
        }
    }

    /// The door part of human_update_hand_grab_and_inventory: in a game, a corporation member within 4 of their base
    /// who looks at its closed garage door opens it, and at the open door's frame closes it.
    pub(crate) fn team_door_probe(&mut self, player: Option<PlayerId>, start: Vec3, end: Vec3, pos: Vec3) {
        let map = &self.world.map;
        let Some(hit) = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, start, end) else { return };
        if hit.area == -1 {
            return;
        }
        let word = map.level.area.collision_cell_resolved(hit.block.x, hit.block.y, hit.block.z) & 0xfcffffff;
        if self.gamestate != GameState::InGame || self.gamemode == GameMode::Eliminator {
            return;
        }
        let Some(k) = self.players.get(player.unwrap_or(PlayerId(0)).idx()).and_then(|p| team_index(p.team)) else { return };
        let base = &map.level.bases[k];
        let (a, b) = (base.interior_min, base.interior_max);
        if pos.x < a.x - DOOR_REACH || b.x + DOOR_REACH <= pos.x || pos.z < a.z - DOOR_REACH || DOOR_REACH + b.z <= pos.z {
            return;
        }
        let garage = map.level.tables.garage_door();
        let frame = (garage[0] | MESH) <= word && word <= (garage[3] | MESH);
        let door = hit.object == GARAGE_DOOR as i32;
        match (self.corp_state[k].door_open, door || frame, frame) {
            (false, true, _) => self.set_team_door(k, true),
            (true, _, true) => self.set_team_door(k, false),
            _ => {}
        }
    }

    fn update_player_event(&mut self, pid: PlayerId) {
        if let Some(p) = self.players.get(pid.idx()) {
            let e = p.make_update_player_event(self.tick);
            self.events.push(e);
        }
    }
}

/// Hooks for checking the corporation menus against the original server.
impl Sim {
    pub fn add_player(&mut self, player: crate::player::Player) -> PlayerId {
        PlayerId(self.players.insert(player) as u32)
    }

    pub fn player(&self, pid: PlayerId) -> Option<&crate::player::Player> {
        self.players.get(pid.idx())
    }

    pub fn player_mut(&mut self, pid: PlayerId) -> Option<&mut crate::player::Player> {
        self.players.get_mut(pid.idx())
    }

    /// One tick of logic_player's menus.
    pub fn run_menus(&mut self) {
        self.update_shop_menus();
    }

    pub fn press_corp_button(&mut self, pid: PlayerId, menu: u8, b: i32) {
        self.corp_menu_action(pid, menu, b);
    }

    pub fn corp(&self, k: usize) -> &CorpState {
        &self.corp_state[k]
    }

    pub fn set_door_cells(&mut self, k: usize, open: bool) {
        self.corp_state[k].door_cells_open = open;
    }

    pub fn corp_mut(&mut self, k: usize) -> &mut CorpState {
        &mut self.corp_state[k]
    }

    /// Puts every base's door cells in line with its door flag.
    pub fn sync_team_doors(&mut self) {
        self.apply_team_doors();
    }

    pub fn probe_team_door(&mut self, player: Option<PlayerId>, start: Vec3, end: Vec3, pos: Vec3) {
        self.team_door_probe(player, start, end, pos);
    }

    pub fn items(&self) -> impl Iterator<Item = (usize, &super::items::Item)> {
        self.items.iter()
    }
}
