use crate::{items::state::ItemState, sim::Sim};

impl Sim {
    /// The key part of logic_item: a key keeps while its vehicle stands, and locks it unless whoever holds the key
    /// sits in it, who then owns it.
    pub(crate) fn key_logic(&mut self, id: usize) {
        let Some(item) = self.items.get(id) else { return };
        let ItemState::Key { vehicle: Some(vid) } = item.state else { return };

        let holder = item.parent_human;
        let active = self.vehicles.get(vid).is_some();

        if active && let Some(i) = self.items.get_mut(id) {
            i.despawn_time = 65535;
        }

        let Some(vehicle) = self.vehicles.get_mut(vid) else { return };
        vehicle.locked = true;

        if let Some(h) = usize::try_from(holder).ok().and_then(|h| self.humans.get(h))
            && h.vehicle == Some(vid)
        {
            vehicle.locked = false;
            vehicle.owner = h.player.map_or(-1, |p| p.0 as i32);
        }
    }
}
