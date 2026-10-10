use std::collections::HashMap;

use glam::IVec3;

const GRID_BLOCKS: IVec3 = IVec3::new(64 * 64, 8 * 64, 64 * 64);
/// The broadphase of collidable items, keyed by 4-unit block (rebuild_item_collision_grid / check_object_collisions).
/// The game keeps 32 items a block and returns 256 a query; this grid has no limit, so a pile of items stays solid.
#[derive(Default)]
pub struct ItemGrid {
    cells: HashMap<IVec3, Vec<usize>>,
}

fn in_grid(c: IVec3) -> bool {
    c.cmpge(IVec3::ZERO).all() && c.cmplt(GRID_BLOCKS).all()
}

impl ItemGrid {
    pub fn clear(&mut self) {
        self.cells.clear();
    }

    pub fn insert(&mut self, id: usize, min: IVec3, max: IVec3) {
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                for x in min.x..=max.x {
                    let c = IVec3::new(x, y, z);
                    if !in_grid(c) {
                        continue;
                    }
                    self.cells.entry(c).or_default().push(id);
                }
            }
        }
    }

    /// Every item touching the blocks `min..=max`, in first-seen order.
    pub fn query(&self, min: IVec3, max: IVec3) -> Vec<usize> {
        let mut out = Vec::new();
        let mut seen = [0u64; super::items::MAX_ITEMS / 64];
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                for x in min.x..=max.x {
                    let c = IVec3::new(x, y, z);
                    if !in_grid(c) {
                        continue;
                    }
                    let Some(cell) = self.cells.get(&c) else { continue };
                    for &id in cell {
                        let (word, bit) = (id / 64, 1u64 << (id % 64));
                        if seen[word] & bit == 0 {
                            seen[word] |= bit;
                            out.push(id);
                        }
                    }
                }
            }
        }
        out
    }
}
