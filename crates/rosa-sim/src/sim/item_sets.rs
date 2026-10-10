use glam::{IVec3, Vec3};
use rosa_protocol::clientbound::game::{
    ItemKind,
    events::{Event, ServerEvent, item_set_cell::EventItemSetCell},
};

use super::Sim;
use crate::world::item_sets::{SetHit, entry_pose};

/// How near its place in the set an item must be when deleted to go back into the set.
const RETURN_REACH: f32 = 0.125;

/// The item-set cell an item was taken from (item +0x318 on): the cell, its word and the item's index in the set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetOrigin {
    pub cell: IVec3,
    pub word: u32,
    pub index: usize,
}

impl Sim {
    pub fn map(&self) -> &crate::world::map::Map {
        &self.world.map
    }

    pub fn set_item_pos(&mut self, id: usize, pos: Vec3) {
        if let Some(item) = self.items.get_mut(id) {
            item.pos2 = pos;
        }
    }

    pub fn reset_level_dynamic(&mut self) {
        self.world.map.level.area.reset_dynamic();
    }

    fn item_set_cell_event(&mut self, cell: IVec3, taken: u32) {
        let e = EventItemSetCell { area: 0, block_x: cell.x, block_y: cell.y, block_z: cell.z, taken, unk: 1 };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::ItemSetCell(e) });
    }

    /// spawn_item_from_grid_cell: item `index` of the set in `cell` (cell word `word`) made a real item where the set
    /// placed it, and marked taken. None when it is already taken or the item table is full.
    pub fn spawn_item_from_grid_cell(&mut self, word: u32, index: usize, cell: IVec3) -> Option<usize> {
        let area = &self.world.map.level.area;
        let bit = 1u32.wrapping_shl(index as u32);
        if area.taken(cell.x, cell.y, cell.z) & bit != 0 {
            return None;
        }
        let e = area.item_sets.set(word)?.entries.get(index)?.clone();
        let (pos, rot) = entry_pose(&e, cell, area.block_size, (word >> 24) & 3);
        let kind = ItemKind::try_from(e.kind as u8).ok()?;
        let id = self.create_item(kind, pos, None, rot)?;
        let area = &mut self.world.map.level.area;
        let taken = area.taken(cell.x, cell.y, cell.z) | bit;
        area.set_taken(cell.x, cell.y, cell.z, taken);
        self.item_set_cell_event(cell, taken);
        if let Some(item) = self.items.get_mut(id) {
            item.set_origin = Some(SetOrigin { cell, word: (word & (3 << 24)) + (word & 1023), index });
        }
        if kind == ItemKind::Computer {
            self.place_set_computer(id, pos, cell);
        }
        Some(id)
    }

    /// Every item of the set a trace hit in `cell` made real (bullet_simulation, item_simulation); the ids by index.
    pub fn spawn_hit_set(&mut self, hit: SetHit, cell: IVec3) -> Vec<(usize, usize)> {
        let count = self.world.map.level.area.item_sets.set(hit.set).map_or(0, |s| s.entries.len());
        let taken = self.world.map.level.area.taken(cell.x, cell.y, cell.z);
        (0..count)
            .filter(|&i| taken & 1u32.wrapping_shl(i as u32) == 0)
            .filter_map(|i| self.spawn_item_from_grid_cell(hit.cell_word(), i, cell).map(|id| (i, id)))
            .collect()
    }

    /// The item-set part of human_update_bones_bbox_wake_items and vehicle_update_bbox_wake_items: every set item in
    /// the cells from `min` to `max` made real, at rest.
    pub fn spawn_set_items_in(&mut self, min: IVec3, max: IVec3) {
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                for x in min.x..=max.x {
                    let area = &self.world.map.level.area;
                    let word = area.item_set(x, y, z);
                    if word == 0 {
                        continue;
                    }
                    let taken = area.taken(x, y, z);
                    let count = area.item_sets.set(word).map_or(0, |s| s.entries.len());
                    for i in 0..count {
                        if taken & 1u32.wrapping_shl(i as u32) != 0 {
                            continue;
                        }
                        let Some(id) = self.spawn_item_from_grid_cell(word, i, IVec3::new(x, y, z)) else { continue };
                        if let Some(item) = self.items.get_mut(id) {
                            item.physics_sim = true;
                            item.physics_settled = true;
                        }
                    }
                }
            }
        }
    }

    /// item_update_grid_footprint_bit: an item deleted close to where its set placed it goes back into the set.
    pub(crate) fn return_set_item(&mut self, id: usize) {
        let Some((o, at)) = self.items.get(id).and_then(|i| Some((i.set_origin?, i.pos2))) else { return };
        let area = &self.world.map.level.area;
        let Some(e) = area.item_sets.set(o.word).and_then(|s| s.entries.get(o.index)) else { return };
        let (pos, _) = entry_pose(e, o.cell, area.block_size, (o.word >> 24) & 3);
        let d = Vec3::new(pos.x - at.x, pos.y - at.y, pos.z - at.z);
        let dist = ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt();
        if !(RETURN_REACH > dist) {
            return;
        }
        let area = &mut self.world.map.level.area;
        let taken = !1u32.wrapping_shl(o.index as u32) & area.taken(o.cell.x, o.cell.y, o.cell.z);
        area.set_taken(o.cell.x, o.cell.y, o.cell.z, taken);
        self.item_set_cell_event(o.cell, taken);
    }
}
