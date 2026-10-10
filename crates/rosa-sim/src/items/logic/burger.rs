use crate::{items::state::ItemState, sim::Sim};

impl Sim {
    pub(crate) fn burger_logic(&mut self, id: usize) {
        let item = self.items.get(id).unwrap();

        let Some(h) = (item.parent_human != -1).then(|| self.humans.get_mut(item.parent_human as usize)).flatten() else { return };

        if h.eat_cooldown != 0 {
            return;
        }

        h.eat_cooldown = 300;
        h.max_stamina = (h.max_stamina + 16).min(255);

        let item = self.items.get_mut(id).unwrap();
        if let ItemState::Burger { bites_left } = &mut item.state {
            *bites_left -= 1;

            if *bites_left <= 0 {
                self.mark_item_for_deletion(id);
            }
        }
    }
}
