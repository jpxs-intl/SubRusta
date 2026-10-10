use rosa_protocol::{
    GameMode, Team,
    clientbound::game::{ItemKind, MenuType, events::chat::ChatType},
};

use super::{Sim, item_state::ItemState};
use crate::PlayerId;

/// The vehicle stock the corporations buy from in round mode (corp_stock_data + 0x1c4, 0x10 each).
pub const VEHICLE_STOCK: usize = 8;
const VEHICLE_COLORS: i32 = 6;
const COLOR_NAMES: [&str; 6] = ["Black", "Red", "Blue", "Silver", "White", "Gold"];
/// A sold-out vehicle slot's button.
const SOLD: i32 = 8;
/// Button bytes 0x80 to 0x85 switch to menus 14 to 19.
const TAB: u32 = 0x80;
const FIRE: i32 = 6;
/// A player can buy 10 items a round day.
const BUY_LIMIT: i32 = 9;
const WEAPONS: [ItemKind; 5] = [ItemKind::Pistol, ItemKind::Mp5, ItemKind::Ak47, ItemKind::M16, ItemKind::Uzi];
const AMMO: [ItemKind; 5] = [ItemKind::PistolMag, ItemKind::Mp5Mag, ItemKind::Ak47Mag, ItemKind::M16Mag, ItemKind::UziMag];
/// The suits on the equipment page: buttons 1 to 3 (colour, price).
const SUITS: [(u8, i32); 3] = [(3, 5000), (8, 20000), (7, 100000)];
/// A corporation records up to 128 fired accounts.
const MAX_FIRED: usize = 128;
const TEAM_NAMES: [&str; 6] = ["Goldmen Inc", "Monsota", "OXS International", "Nexaco", "Pentacom", "Prodocon"];

/// Who a round purchase is for: the buyer's human, or the record of one deleted when they were fired.
#[derive(Clone, Copy, Debug)]
enum Buyer {
    Live(usize),
    Ghost,
}

impl Buyer {
    fn live(self) -> Option<usize> {
        match self {
            Buyer::Live(h) => Some(h),
            Buyer::Ghost => None,
        }
    }
}

/// A fired player's deleted human as the binary keeps it: the record's id and position, and the inventory its
/// purchases go into (empty, as delete_human left it).
#[derive(Clone, Copy, Debug)]
pub struct GhostHuman {
    pub id: usize,
    pub pos: glam::Vec3,
    pub inventory: [crate::human::InventorySlot; crate::human::INVENTORY_SLOTS],
}

/// One vehicle on sale: whether it is still there, its type, colour and price.
#[derive(Clone, Copy, Debug, Default)]
pub struct VehicleOffer {
    pub active: bool,
    pub kind: i32,
    pub color: i32,
    pub price: i32,
}

fn team_index(team: Team) -> Option<usize> {
    Team::CORPORATIONS.iter().position(|&t| t == team)
}

fn menu_type(id: u8) -> MenuType {
    match id {
        14 => MenuType::RoundCorpWeapons,
        15 => MenuType::RoundCorpAmmo,
        16 => MenuType::RoundCorpEquip,
        17 => MenuType::RoundCorpVehicle,
        18 => MenuType::RoundCorpStock,
        _ => MenuType::WorldEmptyCorp,
    }
}

impl Sim {
    /// randomize_corp_vehicle_stock: each vehicle still on sale is sold off with a 1 in 8 chance, then every empty slot
    /// gets a random type and colour at its list price.
    pub(crate) fn randomize_corp_vehicle_stock(&mut self) {
        for o in self.vehicle_stock.iter_mut().filter(|o| o.active) {
            if crate::rng::rand() & 7 == 0 {
                o.active = false;
            }
        }
        for o in self.vehicle_stock.iter_mut().filter(|o| !o.active) {
            let kind = match crate::rng::rand() & 15 {
                0..=1 => 7,
                2..=5 => 0,
                6..=9 => 15,
                10..=12 => 9,
                13..=14 => 6,
                _ => ((crate::rng::rand() & 15) == 0) as i32 + 4,
            };
            let price = self.vehicle_types.get(kind as usize).map_or(0, |t| t.price);
            let color = (crate::rng::rand() as i32) % VEHICLE_COLORS;
            *o = VehicleOffer { active: true, kind, color, price };
        }
    }

    /// The round mode part of logic_player for a human in no shop or bank: a corporation member inside their base gets
    /// the weapons page when no menu is open; anywhere else the menu closes.
    pub(crate) fn round_base_menu(&mut self, pid: PlayerId, pos: glam::Vec3) {
        let at_base = matches!(self.gamemode, GameMode::Round | GameMode::Versus)
            && self.players.get(pid.idx()).and_then(|p| team_index(p.team)).is_some_and(|k| self.world.map.level.bases[k].contains(pos));
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        if !at_base {
            p.menu = MenuType::Empty;
        } else if p.menu == MenuType::Empty {
            p.menu = MenuType::RoundCorpWeapons;
        }
    }

    /// The buttons of the vehicle page (17), "<colour> <type>" priced or SOLD, and of the manager page (19), firing
    /// teammates.
    pub(crate) fn round_menu_buttons(&mut self, pid: PlayerId) {
        let Some(p) = self.players.get(pid.idx()) else { return };
        let mut buttons = Vec::new();
        match p.menu as u8 {
            17 => {
                for (i, o) in self.vehicle_stock.iter().enumerate() {
                    if !o.active {
                        buttons.push((SOLD, "SOLD".to_string(), Some(0)));
                        continue;
                    }
                    let color = usize::try_from(o.color).ok().and_then(|c| COLOR_NAMES.get(c)).copied().unwrap_or("");
                    let name = self.vehicle_types.get(o.kind as usize).map_or("", |t| t.name.as_str());
                    let mut text = format!("{color} {name}");
                    text.truncate(63);
                    buttons.push((i as i32, text, Some(o.price)));
                }
            }
            19 if p.manager_tab => {
                for (i, other) in self.players.iter().filter(|&(i, o)| i != pid.idx() && o.team == p.team) {
                    buttons.push((((i as i32) << 16) + FIRE, format!("Fire {}", other.username), None));
                }
            }
            _ => return,
        }
        if let Some(p) = self.players.get_mut(pid.idx()) {
            for (id, text, extra) in buttons {
                p.push_button(id, &text, extra);
            }
        }
    }

    /// handle_buy_menu_selection: the buy pages of round mode (menus 14 to 19).
    pub(crate) fn round_buy_menu(&mut self, pid: PlayerId, packed: u32) {
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        let b = packed & 255;
        if (p.human.is_some() || p.ghost_human) && (TAB..TAB + 6).contains(&b) {
            p.menu = menu_type((b - TAB + 14) as u8);
        }
        let human = match p.human {
            Some(h) => Buyer::Live(h),
            None if p.ghost_human && p.ghost.is_some() => Buyer::Ghost,
            None => return,
        };
        let target = (packed as i32) >> 16;
        match (p.menu as u8, b) {
            (14, 0..5) => self.round_buy_item(pid, human, WEAPONS[b as usize]),
            (15, 0..5) => self.round_buy_item(pid, human, AMMO[b as usize]),
            (16, 0) => self.round_buy_item(pid, human, ItemKind::Bandage),
            (16, 6) => self.round_buy_item(pid, human, ItemKind::Grenade),
            (16, 7) => self.round_buy_item(pid, human, ItemKind::Briefcase),
            (16, 1..=3) => {
                let (suit, price) = SUITS[b as usize - 1];
                if p.money > price - 1 {
                    p.money -= price;
                    p.customization.suit_color = suit;
                    self.update_human_look(pid, human);
                }
            }
            (16, 4 | 5) => {
                let (necklace, price, enough) = if b == 4 { (1, 10000, p.money > 9999) } else { (2, 1000000, p.money >= 1000000) };
                if enough {
                    p.money -= price;
                    p.customization.necklace = necklace;
                    self.update_human_look(pid, human);
                }
            }
            (17, 0..8) => self.round_buy_vehicle(pid, human, b as usize),
            (18, _) => self.stock_menu_selection(pid, b),
            (19, 6) => self.round_fire(pid, human, target),
            _ => {}
        }
    }

    /// An item for the money, into the first free pocket (a briefcase into a hand).
    fn round_buy_item(&mut self, pid: PlayerId, human: Buyer, kind: ItemKind) {
        let Some(p) = self.players.get(pid.idx()) else { return };
        let price = self.item_types[kind as usize].price;
        if p.items_bought > BUY_LIMIT || !(p.money >= price || price == 0) {
            return;
        }
        let slots = if kind == ItemKind::Briefcase { 0..2 } else { 2..7 };
        self.buyer_give(pid, human, kind, slots, |_| {});
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        p.money -= price;
        if self.gamemode == GameMode::Round {
            let e = p.make_update_round_event(self.tick);
            self.events.push(e);
        }
        if let Some(p) = self.players.get_mut(pid.idx()) {
            p.items_bought += 1;
        }
    }

    fn update_human_look(&mut self, pid: PlayerId, human: Buyer) {
        let Some(p) = self.players.get(pid.idx()) else { return };
        let look = p.customization;
        let e = p.make_update_player_event(self.tick);
        if let Some(h) = human.live().and_then(|h| self.humans.get_mut(h)) {
            h.customization.suit_color = look.suit_color;
            h.customization.necklace = look.necklace;
        }
        self.events.push(e);
    }

    /// A vehicle from the stock, parked in the first free space of the buyer's base (who must be inside it), its key
    /// into a pocket.
    fn round_buy_vehicle(&mut self, pid: PlayerId, human: Buyer, slot: usize) {
        let offer = self.vehicle_stock[slot];
        let Some(p) = self.players.get(pid.idx()) else { return };
        let Some(k) = team_index(p.team) else { return };
        if !offer.active || p.money < offer.price || offer.kind == -1 {
            return;
        }
        let Some(pos) = self.buyer_pos(pid, human) else { return };
        if !self.world.map.level.bases[k].contains(pos) {
            return;
        }
        let Some(vehicle) = self.corporation_spawn_vehicle(offer.kind, k, offer.color) else { return };
        self.buyer_give(pid, human, ItemKind::Key, 2..7, |state| {
            if let ItemState::Key { vehicle: v } = state {
                *v = Some(vehicle);
            }
        });
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        p.money -= offer.price;
        let e = p.make_update_round_event(self.tick);
        self.events.push(e);
        self.vehicle_stock[slot].active = false;
    }

    /// corporation_spawn_vehicle: a vehicle in the first free car space of the corporation's base.
    pub(crate) fn corporation_spawn_vehicle(&mut self, kind: i32, k: usize, color: i32) -> Option<usize> {
        let spaces = self.world.map.level.bases[k].car_spaces.clone();
        let taken = &mut self.corp_state[k].car_space_taken;
        taken.resize(spaces.len(), false);
        let i = taken.iter().position(|t| !t)?;
        taken[i] = true;
        let (pos, rot) = spaces[i];
        self.spawn_vehicle(kind as usize, color, pos, rot)
    }

    /// The manager firing a teammate from inside the base: their weapons, ammo, keys, grenades and bandages despawn,
    /// their human goes, their shares are sold and they leave the corporation.
    fn round_fire(&mut self, pid: PlayerId, human: Buyer, target: i32) {
        let Some(p) = self.players.get(pid.idx()) else { return };
        let Some(k) = team_index(p.team) else { return };
        let team = p.team;
        if self.corp_state[k].manager != Some(pid) {
            return;
        }
        let Some(pos) = self.buyer_pos(pid, human) else { return };
        if !self.world.map.level.bases[k].contains(pos) || self.corp_state[k].fired.len() >= MAX_FIRED || target == pid.0 as i32 {
            return;
        }
        let tid = PlayerId(target as u32);
        let Some(t) = self.players.get(tid.idx()) else { return };
        if t.team != team {
            return;
        }
        self.corp_state[k].fired.push(t.account_id);
        let mut ghost = None;
        if let Some(h) = t.human {
            ghost = self.humans.get(h).map(|g| GhostHuman { id: h, pos: g.pos, inventory: Default::default() });
            self.strip_fired_human(h);
            self.delete_human(h);
        }
        let mode = self.gamemode;
        let Some(t) = self.players.get_mut(tid.idx()) else { return };
        let held = t.stocks;
        super::economy::sell_stocks(t, &mut self.corporations, held, mode);
        t.team = Team::Spectator;
        t.manager_tab = false;
        t.ghost_human = t.human.take().is_some();
        if t.ghost_human {
            t.ghost = ghost;
        }
        let (e, m) = (t.make_update_player_event(self.tick), t.make_update_round_event(self.tick));
        let name = t.username.clone();
        self.events.push(e);
        self.events.push(m);
        self.send_chat(&format!("{name} has been fired from {}", TEAM_NAMES[k]), ChatType::Announce, -1, 0);
    }

    fn buyer_pos(&self, pid: PlayerId, b: Buyer) -> Option<glam::Vec3> {
        match b {
            Buyer::Live(h) => self.humans.get(h).map(|h| h.pos),
            Buyer::Ghost => self.players.get(pid.idx()).and_then(|p| p.ghost.as_ref()).map(|g| g.pos),
        }
    }

    /// give_item for the buyer: into a live human's slots, or a ghost's, where the binary links it into the deleted
    /// human's record and the item stays where it was made.
    fn buyer_give(&mut self, pid: PlayerId, b: Buyer, kind: ItemKind, slots: std::ops::Range<usize>, init: impl FnOnce(&mut ItemState)) -> Option<usize> {
        let ghost = match b {
            Buyer::Live(h) => return self.give_item(h, kind as u8, slots, init),
            Buyer::Ghost => self.players.get(pid.idx())?.ghost?,
        };
        let item = self.create_item(kind, ghost.pos, None, rosa_physics::rotation::IDENTITY)?;
        init(&mut self.items.get_mut(item)?.state);
        let is_gun = self.item_types[kind as usize].is_gun;
        let g = self.players.get_mut(pid.idx())?.ghost.as_mut()?;
        for slot in slots {
            let count = g.inventory[slot].count;
            if count > 1 || (slot <= 1 && count == 1) || (slot == 2 && !is_gun) || (slot > 2 && is_gun) {
                continue;
            }
            if count <= 7 {
                g.inventory[slot].items[count as usize] = item as i32;
                g.inventory[slot].count = count + 1;
                let i = self.items.get_mut(item)?;
                i.parent_human = g.id as i32;
                i.parent_slot = slot as i32;
            }
            break;
        }
        Some(item)
    }

    fn strip_fired_human(&mut self, h: usize) {
        let Some(slots) = self.humans.get(h).map(|h| h.inventory) else { return };
        for slot in slots {
            for &id in &slot.items[..slot.count.max(0) as usize] {
                let Some(item) = self.items.get_mut(id as usize) else { continue };
                let ty = &self.item_types[item.item_type as usize];
                let kind = item.item_type;
                if !(ty.is_gun || ty.magazine_ammo > 0 || matches!(kind, ItemKind::Key | ItemKind::Grenade | ItemKind::Bandage)) {
                    continue;
                }
                item.despawn_time = 0;
                if let Some(&child) = item.children.first() {
                    if let Some(c) = self.items.get_mut(child) {
                        c.despawn_time = 0;
                    }
                    super::items::remove_link(&mut self.items, child, id as usize);
                }
            }
        }
    }
}

/// Hooks for checking the round buy menus against the original server.
impl Sim {
    pub fn press_buy_button(&mut self, pid: PlayerId, packed: u32) {
        self.round_buy_menu(pid, packed);
    }

    pub fn restock_round_vehicles(&mut self) {
        self.randomize_corp_vehicle_stock();
    }

    pub fn vehicle_offers(&self) -> &[VehicleOffer] {
        &self.vehicle_stock
    }

    pub fn vehicle_count(&self) -> usize {
        self.vehicles.iter().count()
    }
}

impl Sim {
    pub fn world_bases(&self) -> &[crate::world::building::CorporationBase] {
        &self.world.map.level.bases
    }
}
