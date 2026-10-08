use std::collections::HashMap;

use glam::IVec3;

const GRID_BLOCKS: IVec3 = IVec3::new(64 * 64, 8 * 64, 64 * 64);
const CELL_CAPACITY: usize = 32;
const MAX_RESULTS: usize = 256;

/// The broadphase of collidable items, keyed by 4-unit block (rebuild_item_collision_grid / check_object_collisions).
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
                    let cell = self.cells.entry(c).or_default();
                    if cell.len() < CELL_CAPACITY {
                        cell.push(id);
                    }
                }
            }
        }
    }

    /// Every item touching the blocks `min..=max`, in first-seen order.
    pub fn query(&self, min: IVec3, max: IVec3) -> Vec<usize> {
        let mut out = Vec::new();
        for y in min.y..=max.y {
            for z in min.z..=max.z {
                for x in min.x..=max.x {
                    let c = IVec3::new(x, y, z);
                    if !in_grid(c) {
                        continue;
                    }
                    let Some(cell) = self.cells.get(&c) else { continue };
                    for &id in cell {
                        if out.len() < MAX_RESULTS && !out.contains(&id) {
                            out.push(id);
                        }
                    }
                }
            }
        }
        out
    }
}
