use std::collections::HashMap;

use glam::{IVec3, Vec3};

pub const CELLS: usize = 512;

pub const TYPE_MASK: u32 = 7 << 29;
pub const CUBE: u32 = 2 << 29;
pub const MESH: u32 = 4 << 29;
pub const FOOTPRINT: u32 = 6 << 29;
pub const ALL_FACES: u8 = 255;

pub struct BlockRecord {
    pub pos: IVec3,
    pub layer0: [u32; CELLS],
    pub layer1: [u32; CELLS],
    pub object: [u32; CELLS],
    pub item_set: [u32; CELLS],
    pub taken: [u32; CELLS],
}

impl BlockRecord {
    fn new(pos: IVec3) -> Box<Self> {
        Box::new(Self {
            pos,
            layer0: [0; CELLS],
            layer1: [0; CELLS],
            object: [0; CELLS],
            item_set: [0; CELLS],
            taken: [0; CELLS],
        })
    }
}

#[inline]
pub fn cell_index(x: i32, y: i32, z: i32) -> usize {
    (((y & 7) << 6) | ((z & 7) << 3) | (x & 7)) as usize
}

pub trait BlockDims {
    fn dims(&self, id: u32) -> IVec3;
}

pub struct AreaGrid {
    pub origin: Vec3,
    pub block_size: f32,
    pub chunks: IVec3,
    records: Vec<Box<BlockRecord>>,
    index: HashMap<IVec3, u32>,
    /// line_intersect_result.unk21: the face of the last custom shape a capsule touched (its wall index | 0x10000 for
    /// a breakable wall). The binary keeps it in a global that only custom shape hits overwrite.
    pub face_attr: std::cell::Cell<u32>,
}

impl AreaGrid {
    pub fn new(chunks: IVec3, origin: Vec3, block_size: f32) -> Self {
        Self { origin, block_size, chunks, records: Vec::new(), index: HashMap::new(), face_attr: std::cell::Cell::new(0) }
    }

    pub fn records(&self) -> &[Box<BlockRecord>] {
        &self.records
    }

    #[inline]
    fn in_range(&self, x: i32, y: i32, z: i32) -> bool {
        x >= 0 && y >= 0 && z >= 0 && (x >> 6) < self.chunks.x && (y >> 6) < self.chunks.y && (z >> 6) < self.chunks.z
    }

    pub fn record(&self, x: i32, y: i32, z: i32) -> Option<&BlockRecord> {
        if !self.in_range(x, y, z) {
            return None;
        }
        self.index.get(&IVec3::new(x >> 3, y >> 3, z >> 3)).map(|&i| &*self.records[i as usize])
    }

    fn record_mut(&mut self, x: i32, y: i32, z: i32) -> Option<&mut BlockRecord> {
        if !self.in_range(x, y, z) {
            return None;
        }
        let key = IVec3::new(x >> 3, y >> 3, z >> 3);
        let i = match self.index.get(&key) {
            Some(&i) => i,
            None => {
                let i = self.records.len() as u32;
                self.records.push(BlockRecord::new(key));
                self.index.insert(key, i);
                i
            }
        };
        Some(&mut self.records[i as usize])
    }

    pub fn layer0(&self, x: i32, y: i32, z: i32) -> u32 {
        self.record(x, y, z).map_or(0, |r| r.layer0[cell_index(x, y, z)])
    }

    pub fn layer1(&self, x: i32, y: i32, z: i32) -> u32 {
        self.record(x, y, z).map_or(0, |r| r.layer1[cell_index(x, y, z)])
    }

    /// area_get_collision_cell_resolved: the layer 1 word of a cell, a footprint's resolved to its origin cell.
    pub fn collision_cell_resolved(&self, x: i32, y: i32, z: i32) -> u32 {
        let v = self.layer1(x, y, z);
        if v & TYPE_MASK != FOOTPRINT {
            return v;
        }
        self.layer1(x - (v & 255) as i32, y - ((v >> 16) & 255) as i32, z - ((v >> 8) & 255) as i32)
    }

    pub fn set_layer0(&mut self, x: i32, y: i32, z: i32, v: u32) {
        if let Some(r) = self.record_mut(x, y, z) {
            r.layer0[cell_index(x, y, z)] = v;
        }
    }

    pub fn set_layer1(&mut self, x: i32, y: i32, z: i32, v: u32) {
        if let Some(r) = self.record_mut(x, y, z) {
            r.layer1[cell_index(x, y, z)] = v;
        }
    }

    pub fn set_object(&mut self, x: i32, y: i32, z: i32, v: u32) {
        if !self.in_range(x, y, z) {
            return;
        }
        if let Some(&i) = self.index.get(&IVec3::new(x >> 3, y >> 3, z >> 3)) {
            self.records[i as usize].object[cell_index(x, y, z)] = v;
        }
    }

    pub fn set_item_set(&mut self, x: i32, y: i32, z: i32, v: u32) {
        if let Some(r) = self.record_mut(x, y, z) {
            let c = cell_index(x, y, z);
            r.item_set[c] = v;
            r.taken[c] = 0;
        }
    }

    pub fn create_block(&mut self, x: i32, y: i32, z: i32, v: u32, faces: u8, dims: &dyn BlockDims) {
        if x < 0 || y < 0 || z < 0 || v == 0 {
            return;
        }
        if self.record_mut(x, y, z).is_none() {
            return;
        }
        match v & TYPE_MASK {
            CUBE => {
                let Some(r) = self.record_mut(x, y, z) else { return };
                let c = cell_index(x, y, z);
                let stored = (v & (3 << 24)) | CUBE | 63 | (faces as u32) << 6;

                r.layer0[c] = stored;

                let below = self.layer0(x, y - 1, z);
                if below & TYPE_MASK == CUBE || y == 0 {
                    self.and_layer0(x, y, z, !1);
                    self.set_layer0(x, y - 1, z, below & !32);
                }

                for (d, own, theirs) in [
                    (IVec3::new(0, 1, 0), 32u32, 1u32),
                    (IVec3::new(0, 0, 1), 2, 8),
                    (IVec3::new(0, 0, -1), 8, 2),
                    (IVec3::new(-1, 0, 0), 4, 16),
                    (IVec3::new(1, 0, 0), 16, 4),
                ] {
                    let (nx, ny, nz) = (x + d.x, y + d.y, z + d.z);
                    let n = self.layer0(nx, ny, nz);

                    if n & TYPE_MASK == CUBE {
                        self.and_layer0(x, y, z, !own);
                        self.set_layer0(nx, ny, nz, n & !theirs);
                    }
                }
            }

            MESH => {
                let Some(r) = self.record_mut(x, y, z) else { return };
                let c = cell_index(x, y, z);
                r.layer0[c] = v;
                self.footprint(x, y, z, v, dims, Self::set_layer0);
            }

            _ => {}
        }
    }

    pub fn place_with_footprint(&mut self, x: i32, y: i32, z: i32, v: u32, dims: &dyn BlockDims) {
        if x < 0 || y < 0 || z < 0 {
            return;
        }

        let Some(r) = self.record_mut(x, y, z) else { return };

        if v & TYPE_MASK != MESH {
            return;
        }

        let c = cell_index(x, y, z);

        r.layer1[c] = v;

        self.footprint(x, y, z, v, dims, Self::set_layer1);
    }

    fn footprint(&mut self, x: i32, y: i32, z: i32, v: u32, dims: &dyn BlockDims, set: fn(&mut Self, i32, i32, i32, u32)) {
        let d = dims.dims(v & 65535);
        for i in 0..d.y {
            for j in 0..d.z {
                for r in 0..d.x {
                    if i | j | r == 0 {
                        continue;
                    }

                    set(self, x + r, y + i, z + j, FOOTPRINT | r as u32 | (j as u32) << 8 | (i as u32) << 16);
                }
            }
        }
    }

    pub fn clear_cell(&mut self, x: i32, y: i32, z: i32) {
        if !self.in_range(x, y, z) {
            return;
        }

        if let Some(&i) = self.index.get(&IVec3::new(x >> 3, y >> 3, z >> 3)) {
            let c = cell_index(x, y, z);
            self.records[i as usize].layer0[c] = 0;
            self.records[i as usize].layer1[c] = 0;
        }
    }

    fn and_layer0(&mut self, x: i32, y: i32, z: i32, mask: u32) {
        if let Some(r) = self.record_mut(x, y, z) {
            r.layer0[cell_index(x, y, z)] &= mask;
        }
    }
}
