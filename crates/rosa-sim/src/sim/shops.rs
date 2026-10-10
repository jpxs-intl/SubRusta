use rosa_physics::rotation::IDENTITY;
use rosa_protocol::clientbound::game::MenuType;

use rosa_protocol::clientbound::game::ItemKind;

use super::{Sim, item_types::ItemType};
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
/// The clothing store's buttons: three suits (colours 3, 8 and 7) and two necklaces.
const SUITS: [u8; 3] = [3, 8, 7];
const POCKETS: std::ops::Range<usize> = 2..7;
const ANY_SLOT: std::ops::Range<usize> = 0..7;

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
                        // TODO: dealership_buy_vehicle (spawn the car at the dealership with its key), then the front page
                        CAR_DEALER => {}
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
                // TODO: player_sell_vehicle (button 1)
                if button == BACK
                    && let Some(p) = self.players.get_mut(pid.idx())
                {
                    p.menu = MenuType::WorldCarShop;
                }
            }
            _ => {}
        }
    }

    fn shop_entry(&self, pid: PlayerId, button: u32) -> Option<ShopEntry> {
        let p = self.players.get(pid.idx())?;
        let b = self.world.map.level.buildings.get(usize::try_from(p.menu_tab).ok()?)?;
        b.shop.get((button as usize).checked_sub(1)?).copied()
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
