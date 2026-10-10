use glam::Vec3;
use rosa_physics::rotation::{IDENTITY, rotate_orientation};
use rosa_protocol::clientbound::game::MenuType;

use rosa_protocol::clientbound::game::ItemKind;

use super::{Sim, item_state::{BILL_VALUES, ItemState}, item_types::ItemType};
use crate::{
    PlayerId,
    vehicle::types::VehicleType,
    world::building::{BURGER_SHOP, BuildingRecord, CAR_DEALER, CLOTHING_STORE, GUN_STORE, ShopEntry},
};

/// A gun store's stock (init_car_dealership 0x427040, run at reset_game and every 18000 clock units), each at its item
/// type's price.
const GUN_STORE_STOCK: [u8; 12] = [11, 12, 7, 8, 1, 2, 3, 4, 9, 10, 14, 13];
/// A dealership holds up to eight cars; each tick of a restock a car is sold off with a 1 in 8 chance.
const DEALERSHIP_SLOTS: usize = 8;
const VEHICLE_COLORS: i32 = 6;
/// The burger shop is stocked while the world clock is before closing time.
const SHOPS_CLOSE: i32 = 0x3e9f3f;
const BURGER_PRICE: i32 = 20;
/// A player can buy 20 items from a gun store, or 10 burgers.
const GUN_STORE_LIMIT: i32 = 19;
const BURGER_LIMIT: i32 = 9;
/// The button that leaves a shop's list for its front page.
const BACK: u32 = 16;
const SELL: u32 = 1;
const MAX_VEHICLES_BOUGHT: i32 = 2;
/// The clothing store's buttons: three suits (colours 3, 8 and 7) and two necklaces.
const SUITS: [u8; 3] = [3, 8, 7];
const POCKETS: std::ops::Range<usize> = 2..7;
const ANY_SLOT: std::ops::Range<usize> = 0..7;
/// The bank's buttons: 1 to 6 withdraw a $5 to $1000 bill (the button is the bill's code), 7 deposits the cash in hand.
const WITHDRAW_BILLS: std::ops::RangeInclusive<u32> = 1..=6;
const DEPOSIT: u32 = 7;
/// $5 and $10 bills: a player can take 40 of them a day.
const SMALL_BILL: u32 = 2;
const SMALL_BILL_LIMIT: i32 = 39;

fn menu(id: u8) -> MenuType {
    match id {
        9 => MenuType::WorldCarShop,
        10 => MenuType::WorldStore,
        11 => MenuType::WorldStoreDone,
        13 => MenuType::WorldBank2,
        _ => MenuType::Empty,
    }
}

impl Sim {
    pub(crate) fn restock_dealerships(&mut self) {
        restock_dealerships(&mut self.world.map.level.buildings, &self.vehicle_types);
    }

    pub(crate) fn stock_gun_stores(&mut self) {
        stock_gun_stores(&mut self.world.map.level.buildings, &self.item_types);
    }

    /// The shop part of logic_world each tick: before closing time every burger shop has its burger.
    pub(crate) fn stock_burger_shops(&mut self) {
        if self.world_time > SHOPS_CLOSE {
            return;
        }
        for b in self.world.map.level.buildings.iter_mut().filter(|b| b.kind == BURGER_SHOP) {
            b.shop = vec![ShopEntry { kind: 0, price: BURGER_PRICE, extra: 0 }];
        }
    }

    /// The menu part of logic_player: standing in a dealership opens its front page (menu 9), in a store, clothing
    /// store or burger shop its list (menu 10), in a bank the bank (menu 13); leaving the building closes menus 9 to 11.
    /// In world mode a human in none of them gets the corporation base menus, and every player's buttons are rebuilt.
    pub(crate) fn update_shop_menus(&mut self) {
        let world = self.gamemode == rosa_protocol::GameMode::World;
        for pid in self.players.iter().map(|(_, p)| p.player_id).collect::<Vec<_>>() {
            let outside = self.shop_menu(pid);
            if world {
                if let Some(pos) = outside.flatten() {
                    self.corp_base_menu(pid, pos);
                }
                self.corp_tab_menu(pid);
            } else {
                if let Some(pos) = outside.flatten() {
                    self.round_base_menu(pid, pos);
                }
                self.round_menu_buttons(pid);
            }
        }
        if world {
            self.copy_corp_money();
        }
    }

    /// One player's shop menu. Returns None for a player without a human, and the human's position when it is in no
    /// shop or bank.
    fn shop_menu(&mut self, pid: PlayerId) -> Option<Option<glam::Vec3>> {
        let buildings = &self.world.map.level.buildings;
        let restarting = self.gamestate == rosa_protocol::clientbound::game::GameState::Restarting;
        let p = self.players.get_mut(pid.idx())?;
        p.menu_buttons.clear();
        if p.ghost_human && p.human.is_none() {
            p.menu = MenuType::Empty;
            return None;
        }
        if p.human.is_none() {
            use rosa_protocol::GameMode as M;
            if matches!(self.gamemode, M::Racing | M::Round | M::Eliminator | M::CoOp | M::Versus) {
                p.menu = if self.gamestate == rosa_protocol::clientbound::game::GameState::Intermission { MenuType::Lobby } else { MenuType::Empty };
            }
            return None;
        }
        let pos = p.human.and_then(|h| self.humans.get(h)).map(|h| h.pos)?;
        if restarting {
            p.menu = MenuType::Empty;
            return Some(None);
        }
        let id = p.menu as u8;
        if (9..=11).contains(&id) && !usize::try_from(p.menu_tab).ok().and_then(|k| buildings.get(k)).is_some_and(|b| b.contains(pos, 0.0)) {
            p.menu = MenuType::Empty;
        }
        let shop = buildings.iter().enumerate().find_map(|(k, b)| match b.kind {
            CAR_DEALER if b.contains(pos, 0.0) => Some((k, 9)),
            GUN_STORE | BURGER_SHOP | CLOTHING_STORE if b.contains(pos, 0.0) => Some((k, 10)),
            _ => None,
        });
        let found = shop.or_else(|| buildings.iter().position(|b| b.kind == crate::world::building::BANK && b.contains(pos, 0.0)).map(|k| (k, 13)));
        let Some((k, m)) = found else {
            // TODO: the binary closes every menu here in world mode; the /stocks test menu is kept open
            return Some((p.menu != MenuType::RoundCorpStock).then_some(pos));
        };
        if p.menu == MenuType::Empty {
            p.menu = menu(m);
            p.menu_tab = k as i32;
        }
        Some(None)
    }

    /// The shop menus of logic_playerinteractions: the front page picks buying (1) or selling (2); a list buys its entry
    /// `button`, and the back button returns to the front page.
    pub(crate) fn shop_menu_action(&mut self, pid: PlayerId, button: u32) {
        let Some(p) = self.players.get(pid.idx()) else { return };
        if p.human.is_none() {
            return;
        }
        match p.menu as u8 {
            9 => {
                let p = self.players.get_mut(pid.idx()).unwrap();
                match button {
                    1 => p.menu = MenuType::WorldStore,
                    2 => p.menu = MenuType::WorldStoreDone,
                    _ => {}
                }
            }
            10 => {
                let Some(b) = usize::try_from(p.menu_tab).ok().and_then(|k| self.world.map.level.buildings.get(k)) else { return };
                let (kind, count) = (b.kind, b.shop.len() as u32);
                if button > 0 && button <= count {
                    match kind {
                        CLOTHING_STORE => self.clothing_store_purchase(pid, button),
                        CAR_DEALER => self.dealership_buy_vehicle(pid, button),
                        GUN_STORE if p.items_bought <= GUN_STORE_LIMIT => self.buy_shop_item(pid, button),
                        BURGER_SHOP => self.burger_shop_menu(pid, button),
                        _ => {}
                    }
                }
                if button == BACK
                    && let Some(p) = self.players.get_mut(pid.idx())
                {
                    p.menu = MenuType::WorldCarShop;
                }
            }
            11 => {
                if button == SELL {
                    self.player_sell_vehicle(pid);
                }
                if button == BACK
                    && let Some(p) = self.players.get_mut(pid.idx())
                {
                    p.menu = MenuType::WorldCarShop;
                }
            }
            _ => {}
        }
    }

    /// The bank menu (13) of logic_playerinteractions; any other button (16) leaves it.
    pub(crate) fn bank_menu_action(&mut self, pid: PlayerId, button: u32) {
        let Some(human) = self.players.get(pid.idx()).and_then(|p| p.human) else { return };
        match button {
            b if WITHDRAW_BILLS.contains(&b) => self.bank_withdraw(pid, human, b),
            DEPOSIT => self.bank_deposit(pid, human),
            BACK => {
                if let Some(p) = self.players.get_mut(pid.idx()) {
                    p.menu = MenuType::EmptyBase;
                }
            }
            _ => {}
        }
    }

    /// A bill of `code` from the player's money: new cash in an empty right hand, or onto the cash held there; the
    /// money goes even when no cash could be made.
    fn bank_withdraw(&mut self, pid: PlayerId, human: usize, code: u32) {
        let value = BILL_VALUES[code as usize];
        let Some(p) = self.players.get(pid.idx()) else { return };
        if p.money < value || (code <= SMALL_BILL && p.bills_withdrawn > SMALL_BILL_LIMIT) {
            return;
        }
        let Some(hand) = self.humans.get(human).map(|h| h.inventory[0]) else { return };
        if hand.count <= 0 {
            self.give_item(human, ItemKind::CashWorld as u8, 0..1, |state| {
                if let ItemState::Cash(cash) = state {
                    cash.bills = 0;
                    cash.codes = code;
                }
            });
        } else {
            let Some(cash) = self.items.get_mut(hand.items[0] as usize).filter(|i| i.item_type == ItemKind::CashWorld).and_then(|i| i.state.cash_mut()) else { return };
            if !cash.insert(0, code) {
                return;
            }
        }
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        p.money -= value;
        if code <= SMALL_BILL {
            p.bills_withdrawn += 1;
        }
    }

    /// The cash in the right hand goes into the player's money.
    fn bank_deposit(&mut self, pid: PlayerId, human: usize) {
        let Some(hand) = self.humans.get(human).map(|h| h.inventory[0]) else { return };
        if hand.count <= 0 {
            return;
        }
        let id = hand.items[0] as usize;
        let Some(value) = self.items.get(id).filter(|i| i.item_type == ItemKind::CashWorld).and_then(|i| i.state.cash()).map(|c| c.value()) else { return };
        if let Some(p) = self.players.get_mut(pid.idx()) {
            p.money += value;
        }
        self.mark_item_for_deletion(id);
    }

    /// dealership_buy_vehicle: a player with a human and fewer than 3 cars bought this round buys the car on list line
    /// `button`: it is put in the dealership's next space, turned a quarter from the building, owned by the player,
    /// with its key in the first free pocket, and the line goes (the last one takes its place).
    fn dealership_buy_vehicle(&mut self, pid: PlayerId, button: u32) {
        let Some(p) = self.players.get(pid.idx()) else { return };
        let Some(human) = p.human else { return };
        if p.vehicles_bought > MAX_VEHICLES_BOUGHT {
            return;
        }
        let Some(k) = usize::try_from(p.menu_tab).ok() else { return };
        let money = p.money;
        let Some(b) = self.world.map.level.buildings.get_mut(k) else { return };
        let Some(e) = b.shop.get(button as usize - 1).copied() else { return };
        if money < e.price {
            return;
        }
        let n = b.car_counter;
        let d = (-n) as f32 * 4.0;
        let r2 = b.rot[2];
        let s = b.car_spawn;
        let pos = Vec3::new(r2.x * d + s.x, r2.y * d + s.y, d * r2.z + s.z);
        b.car_counter = (n + 1) & 7;
        let mut rot = b.rot;
        rotate_orientation(&mut rot, Vec3::Y, 90.0_f32.to_radians());
        let kind = u8::try_from(e.kind).ok().and_then(|k| rosa_protocol::clientbound::game::VehicleKind::try_from(k).ok());
        let vid = kind.and_then(|kind| self.spawn_vehicle(kind, e.extra, pos, rot));
        if let Some(v) = vid.and_then(|v| self.vehicles.get_mut(v)) {
            v.owner = pid.0 as i32;
        }
        if let Some(p) = self.players.get_mut(pid.idx()) {
            p.money -= e.price;
        }
        if let Some(key) = self.create_item(ItemKind::Key, pos, None, rot) {
            if let Some(i) = self.items.get_mut(key) {
                i.state = super::item_state::ItemState::Key { vehicle: vid };
            }
            self.link_first_free(human, key, POCKETS);
        }
        if let Some(p) = self.players.get_mut(pid.idx()) {
            p.vehicles_bought += 1;
            let e = p.make_update_player_event(self.tick);
            self.events.push(e);
        }
        if self.gamemode == rosa_protocol::GameMode::Round
            && let Some(p) = self.players.get(pid.idx())
        {
            let e = p.make_update_round_event(self.tick);
            self.events.push(e);
        }
        if let Some(b) = self.world.map.level.buildings.get_mut(k) {
            b.shop.swap_remove(button as usize - 1);
        }
    }

    /// player_sell_vehicle: the key in the right hand sells its standing vehicle for three quarters of its price; the
    /// car and the key go.
    fn player_sell_vehicle(&mut self, pid: PlayerId) {
        let Some(h) = self.players.get(pid.idx()).and_then(|p| p.human).and_then(|h| self.humans.get(h)) else { return };
        let hand = &h.inventory[0];
        if hand.count <= 0 {
            return;
        }
        let key = hand.items[0] as usize;
        let Some(super::item_state::ItemState::Key { vehicle: Some(vid) }) = self.items.get(key).map(|i| i.state.clone()) else { return };
        let Some(v) = self.vehicles.get(vid).filter(|v| v.health > 0) else { return };
        let price = self.vehicle_types.get(v.kind as usize).map_or(0, |t| t.price);
        if let Some(p) = self.players.get_mut(pid.idx()) {
            p.money = (price as f32 * 0.75 + p.money as f32) as i32;
        }
        if let Some(v) = self.vehicles.get_mut(vid) {
            v.despawn_time = 0;
        }
        if let Some(i) = self.items.get_mut(key) {
            i.despawn_time = 0;
        }
    }

    fn shop_entry(&self, pid: PlayerId, button: u32) -> Option<ShopEntry> {
        let p = self.players.get(pid.idx())?;
        let b = self.world.map.level.buildings.get(usize::try_from(p.menu_tab).ok()?)?;
        b.shop.get((button as usize).checked_sub(1)?).copied()
    }

    /// The item put into the first of `slots` of the human that takes it.
    fn link_first_free(&mut self, human: usize, item: usize, slots: std::ops::Range<usize>) {
        let Sim { humans, bodies, item_grid, items, item_types, vehicles, vehicle_types, .. } = self;
        let Some(h) = humans.get_mut(human) else { return };
        let mut touch = super::items::Touchables { grid: item_grid, items, types: item_types, vehicles, vehicle_types, occupied: Vec::new() };
        for slot in slots {
            if crate::human::inventory::link_item_to_human(h, human, bodies, &mut touch, item, slot) {
                break;
            }
        }
    }

    /// An item at the human, set up by `init` and put into the first of `slots` that takes it. Returns the item.
    pub(super) fn give_item(&mut self, human: usize, kind: u8, slots: std::ops::Range<usize>, init: impl FnOnce(&mut super::item_state::ItemState)) -> Option<usize> {
        let kind = ItemKind::try_from(kind).ok()?;
        let pos = self.humans.get(human).map(|h| h.pos)?;
        let item = self.create_item(kind, pos, None, IDENTITY)?;
        if let Some(i) = self.items.get_mut(item) {
            init(&mut i.state);
        }
        let Sim { humans, bodies, item_grid, items, item_types, vehicles, vehicle_types, .. } = self;
        let Some(h) = humans.get_mut(human) else { return Some(item) };
        let mut touch = super::items::Touchables { grid: item_grid, items, types: item_types, vehicles, vehicle_types, occupied: Vec::new() };
        for slot in slots {
            if crate::human::inventory::link_item_to_human(h, human, bodies, &mut touch, item, slot) {
                break;
            }
        }
        Some(item)
    }

    /// buy_shop_item: the entry's item into the first free pocket, paid for.
    fn buy_shop_item(&mut self, pid: PlayerId, button: u32) {
        let Some(entry) = self.shop_entry(pid, button) else { return };
        let Some(p) = self.players.get(pid.idx()) else { return };
        if entry.price > p.money {
            return;
        }
        if let Some(h) = p.human {
            self.give_item(h, entry.kind as u8, POCKETS, |_| {});
        }
        let p = self.players.get_mut(pid.idx()).unwrap();
        p.money -= entry.price;
        p.items_bought += 1;
        let e = p.make_update_player_event(self.tick);
        self.events.push(e);
    }

    /// burger_shop_menu: a burger (button 1) into the first slot that takes it, paid for.
    fn burger_shop_menu(&mut self, pid: PlayerId, button: u32) {
        let Some(entry) = self.shop_entry(pid, button) else { return };
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        if p.items_bought > BURGER_LIMIT || p.money < entry.price {
            return;
        }
        p.money -= entry.price;
        p.items_bought += 1;
        if let Some(h) = p.human
            && button == 1
        {
            self.give_item(h, ItemKind::Burger as u8, ANY_SLOT, |_| {});
        }
    }

    /// clothing_store_purchase: buttons 1 to 3 change the suit's colour, 4 and 5 the necklace, each paid for only when
    /// it changes something.
    fn clothing_store_purchase(&mut self, pid: PlayerId, button: u32) {
        let Some(entry) = self.shop_entry(pid, button) else { return };
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        let Some(human) = p.human else { return };
        if p.money < entry.price {
            return;
        }
        let mut look = p.customization;
        match button {
            1..=3 => {
                let suit = SUITS[button as usize - 1];
                if look.suit_color != suit {
                    look.suit_color = suit;
                    p.money -= entry.price;
                }
            }
            4 | 5 => {
                let necklace = (button != 4) as u8 + 1;
                if look.necklace != necklace {
                    look.necklace = necklace;
                    p.money -= entry.price;
                }
            }
            _ => {}
        }
        p.customization = look;
        let e = p.make_update_player_event(self.tick);
        if let Some(h) = self.humans.get_mut(human) {
            h.customization.suit_color = look.suit_color;
            h.customization.necklace = look.necklace;
        }
        self.events.push(e);
    }
}

/// restock_shop_vehicles: each dealership sells off some cars at random, then fills up to eight with random types
/// and colours at their list price.
pub fn restock_dealerships(buildings: &mut [BuildingRecord], types: &[VehicleType]) {
    for b in buildings.iter_mut().filter(|b| b.kind == CAR_DEALER) {
        let mut j = 0;

        while j < b.shop.len() {
            if crate::rng::rand() & 7 == 0 {
                let last = b.shop.pop().unwrap();

                if j < b.shop.len() {
                    b.shop[j] = last;
                }
            } else {
                j += 1;
            }
        }

        while b.shop.len() < DEALERSHIP_SLOTS {
            let kind = crate::vehicle::types::random_stock_vehicle() as i32;

            let price = types.get(kind as usize).map_or(0, |t| t.price);
            let extra = (crate::rng::rand() as i32) % VEHICLE_COLORS;

            b.shop.push(ShopEntry { kind, price, extra });
        }
    }
}

/// The gun stores' fixed stock.
pub fn stock_gun_stores(buildings: &mut [BuildingRecord], item_types: &[ItemType]) {
    let stock: Vec<ShopEntry> = GUN_STORE_STOCK.iter().map(|&k| ShopEntry { kind: k as i32, price: item_types[k as usize].price, extra: 0 }).collect();
    
    for b in buildings.iter_mut().filter(|b| b.kind == GUN_STORE) {
        b.shop = stock.clone();
    }
}

impl Sim {
    pub fn buildings(&self) -> &[BuildingRecord] {
        &self.world.map.level.buildings
    }

    pub fn building(&self, k: usize) -> Option<&BuildingRecord> {
        self.world.map.level.buildings.get(k)
    }

    pub fn building_mut(&mut self, k: usize) -> Option<&mut BuildingRecord> {
        self.world.map.level.buildings.get_mut(k)
    }

    pub fn run_dealership_buy(&mut self, pid: PlayerId, button: u32) {
        self.dealership_buy_vehicle(pid, button);
    }

    pub fn run_sell_vehicle(&mut self, pid: PlayerId) {
        self.player_sell_vehicle(pid);
    }

    /// Puts `item` alone in the human's right hand without linking it.
    pub fn force_hand_item(&mut self, h: usize, item: usize) {
        if let Some(hu) = self.humans.get_mut(h) {
            hu.inventory[0].count = 1;
            hu.inventory[0].items[0] = item as i32;
        }
    }
}

impl Sim {
    /// The bank menu's button `button`, pressed with the menu open.
    pub fn run_bank_action(&mut self, pid: PlayerId, button: u32) {
        if let Some(p) = self.players.get_mut(pid.idx()) {
            p.menu = MenuType::WorldBank2;
        }
        self.bank_menu_action(pid, button);
    }
}
