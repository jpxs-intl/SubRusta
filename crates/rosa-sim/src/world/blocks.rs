use glam::IVec3;

use crate::world::area::BlockDims;

pub const CURB: u32 = 15;
pub const CURB_RAMP: u32 = 16;
pub const LAMP: u32 = 20;
pub const WINDOW_FRAME: u32 = 708;
pub const DOOR_FRAME: u32 = 712;
pub const CUSTOM_BASE: u32 = 716;

const SLOPE_BASE: u32 = 66;

pub fn slope_id(width: i32, boxes: bool, row: i32, col: i32) -> Option<u32> {
    if !(4..=12).contains(&width) || !(0..4).contains(&row) || !(0..width).contains(&col) {
        return None;
    }
    let w = width as u32;
    let base = SLOPE_BASE + (4..w).map(|v| 8 * v).sum::<u32>();
    Some(base + if boxes { 4 * w } else { 0 } + row as u32 * w + col as u32)
}

pub struct BlockTable {
    custom_count: u32,
    garage_door: Option<u32>,
    /// A corporation base's table and safe blocks, which mark where the table and vault are.
    base_table: Option<u32>,
    safe: Option<u32>,
    next_id: u32,
}

impl BlockTable {
    pub fn new(custom_names: &[&str]) -> Self {
        let custom_count = custom_names.len() as u32;
        Self {
            custom_count,
            garage_door: custom_names.iter().rposition(|n| *n == "garagedoor").map(Self::custom),
            base_table: custom_names.iter().rposition(|n| *n == "basetable").map(Self::custom),
            safe: custom_names.iter().rposition(|n| *n == "bc-safe01").map(Self::custom),
            next_id: CUSTOM_BASE + 4 * custom_count + 4,
        }
    }

    pub fn custom(k: usize) -> u32 {
        CUSTOM_BASE + 4 * k as u32
    }

    pub fn custom_end(&self) -> u32 {
        CUSTOM_BASE + 4 * self.custom_count
    }

    pub fn edge_cap(&self, i: u32) -> u32 {
        self.custom_end() + i
    }

    /// Whether `id` is one of the four turns of the base table or the safe.
    pub fn is_base_table(&self, id: u32) -> bool {
        self.base_table.is_some_and(|b| (b..=b + 3).contains(&id))
    }

    pub fn is_safe(&self, id: u32) -> bool {
        self.safe.is_some_and(|b| (b..=b + 3).contains(&id))
    }

    pub fn garage_door(&self) -> [u32; 4] {
        self.garage_door.map_or([0; 4], |b| [b, b + 1, b + 2, b + 3])
    }

    pub fn total_block_count(&self) -> u32 {
        self.next_id
    }

    pub fn alloc_block_id(&mut self) -> Option<u32> {
        let id = self.next_id;
        if id > 65535 {
            return None;
        }
        self.next_id += 1;
        Some(id)
    }
}

pub struct StaticDims {
    dims: Vec<IVec3>,
}

impl StaticDims {
    pub fn new(custom_sizes: &[IVec3]) -> Self {
        let mut dims = vec![IVec3::ONE; CUSTOM_BASE as usize];
        for id in (24..=65).chain(642..=707) {
            dims[id] = IVec3::ZERO;
        }

        for w in 4..=12 {
            for row in 0..4 {
                dims[slope_id(w, true, row, w - 1).unwrap() as usize] = IVec3::new(1, 2, 1);
            }
        }

        for s in custom_sizes {
            for o in 0..4 {
                dims.push(if o % 2 == 0 { *s } else { IVec3::new(s.z, s.y, s.x) });
            }
        }
        
        Self { dims }
    }
}

impl BlockDims for StaticDims {
    fn dims(&self, id: u32) -> IVec3 {
        self.dims.get(id as usize).copied().unwrap_or(IVec3::ONE)
    }
}
