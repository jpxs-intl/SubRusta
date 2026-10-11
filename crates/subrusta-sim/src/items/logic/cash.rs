use subrusta_protocol::clientbound::game::ItemKind;

use crate::{
    human::inventory,
    items::{state::Cash, touchables},
    sim::Sim,
};

impl Sim {
    /// The world cash part of logic_playerinteractions: key 0 and 1 move the pick along the stack; a higher key (with
    /// enough bills) hands the picked bill to the other hand, onto its stack or as a stack of its own.
    pub(crate) fn cash_action(&mut self, human: usize, hand: usize, item_id: usize, key: i32) {
        let Some(cash) = self.items.get(item_id).and_then(|i| i.state.cash()).copied() else { return };

        match key {
            0 => self.items.get_mut(item_id).unwrap().state.cash_mut().unwrap().spread += 1,
            1 => self.items.get_mut(item_id).unwrap().state.cash_mut().unwrap().spread -= 1,
            _ if cash.bills >= key - 2 => self.move_bill(human, hand, item_id, cash),
            _ => {}
        }

        let Some(c) = self.items.get_mut(item_id).and_then(|i| i.state.cash_mut()) else { return };
        if c.spread < 0 {
            c.spread = 0;
        }

        if c.bills < c.spread {
            c.spread = c.bills;
        }
    }

    /// The picked bill to the other hand: onto the stack there, or as a new one-bill stack when that hand is empty.
    fn move_bill(&mut self, human: usize, hand: usize, item_id: usize, cash: Cash) {
        let other = hand ^ 1;
        let code = cash.code(cash.spread);
        let held = self.humans.get(human).and_then(|h| (h.inventory[other].count > 0).then_some(h.inventory[other].items[0] as usize));

        match held {
            None => {
                let Some(item) = self.items.get(item_id) else { return };

                let Some((pos, rot)) = self.bodies.get(item.body).map(|b| (b.pos, b.rot)) else { return };

                if let Some(new) = self.create_item(ItemKind::CashWorld, pos, Some(item.vel), rot) {
                    if let Some(c) = self.items.get_mut(new).and_then(|i| i.state.cash_mut()) {
                        c.bills = 0;
                        c.codes = code;
                    }

                    if let Some(h) = self.humans.get_mut(human) {
                        let mut touch = touchables!(self, Vec::new());
                        inventory::link_item_to_human(h, human, &mut self.bodies, &mut touch, new, other);
                    }
                }
            }

            Some(o) => {
                let added = self.items.get_mut(o).and_then(|i| i.state.cash_mut()).is_some_and(|c| c.insert(0, code));

                if !added {
                    return;
                }
            }
        }

        let Some(item) = self.items.get_mut(item_id) else { return };
        let Some(c) = item.state.cash_mut() else { return };

        let spread = c.spread;

        if !c.remove(spread) {
            item.despawn_time = 0;
        }
    }
}
