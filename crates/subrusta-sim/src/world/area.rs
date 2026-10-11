use glam::{IVec3, Vec3};

pub const CELLS: usize = 512;

pub const TYPE_MASK: u32 = 7 << 29;
pub const CUBE: u32 = 2 << 29;
pub const MESH: u32 = 4 << 29;
pub const FOOTPRINT: u32 = 6 << 29;
pub const ALL_FACES: u8 = 255;
/// No page or record in an index slot.
const NONE: u32 = u32::MAX;
/// A 64-cell chunk holds 8 x 8 x 8 blocks of 8 cells.
const PAGE: usize = 512;

pub struct BlockRecord {
    pub layer0: [u32; CELLS],
    pub layer1: [u32; CELLS],
    pub object: [u32; CELLS],
    pub item_set: [u32; CELLS],
    pub taken: [u32; CELLS],
}

impl BlockRecord {
    fn new() -> Box<Self> {
        Box::new(Self { layer0: [0; CELLS], layer1: [0; CELLS], object: [0; CELLS], item_set: [0; CELLS], taken: [0; CELLS] })
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
    /// Each 64-cell chunk's page in `block_records`, or NONE for a chunk without blocks.
    chunk_pages: Vec<u32>,
    /// Per page, the record of each of the chunk's 512 blocks, or NONE.
    block_records: Vec<u32>,
    /// line_intersect_result.unk21: the face of the last custom shape a capsule touched (its wall index | 0x10000 for
    /// a breakable wall). The binary keeps it in a global that only custom shape hits overwrite.
    pub face_attr: u32,
    /// The item sets the level's item-set cells place, and the item hulls traces test them against.
    pub item_sets: crate::world::item_sets::ItemSets,
    /// The dynamic meshes reset_game builds on test2 (the race track and ramps).
    pub track: crate::world::track::Track,
}

impl AreaGrid {
    pub(crate) fn new(chunks: IVec3, origin: Vec3, block_size: f32) -> Self {
        Self {
            origin,
            block_size,
            chunks,
            records: Vec::new(),
            chunk_pages: vec![NONE; (chunks.x * chunks.y * chunks.z) as usize],
            block_records: Vec::new(),
            face_attr: 0,
            item_sets: Default::default(),
            track: Default::default(),
        }
    }

    #[inline]
    fn chunk_slot(&self, x: i32, y: i32, z: i32) -> usize {
        (((x >> 6) * self.chunks.y + (y >> 6)) * self.chunks.z + (z >> 6)) as usize
    }

    #[inline]
    fn block_slot(x: i32, y: i32, z: i32) -> usize {
        ((((x >> 3) & 7) << 6) | (((y >> 3) & 7) << 3) | ((z >> 3) & 7)) as usize
    }

    /// The record of the block holding cell (x, y, z), if it has one.
    #[inline]
    fn record_id(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        if !self.in_range(x, y, z) {
            return None;
        }
        let page = self.chunk_pages[self.chunk_slot(x, y, z)];
        if page == NONE {
            return None;
        }
        let id = self.block_records[page as usize * PAGE + Self::block_slot(x, y, z)];
        (id != NONE).then_some(id as usize)
    }

    #[inline]
    fn in_range(&self, x: i32, y: i32, z: i32) -> bool {
        x >= 0 && y >= 0 && z >= 0 && (x >> 6) < self.chunks.x && (y >> 6) < self.chunks.y && (z >> 6) < self.chunks.z
    }

    pub(crate) fn record(&self, x: i32, y: i32, z: i32) -> Option<&BlockRecord> {
        self.record_id(x, y, z).map(|i| &*self.records[i])
    }

    fn record_mut(&mut self, x: i32, y: i32, z: i32) -> Option<&mut BlockRecord> {
        if !self.in_range(x, y, z) {
            return None;
        }
        let chunk = self.chunk_slot(x, y, z);
        if self.chunk_pages[chunk] == NONE {
            self.chunk_pages[chunk] = (self.block_records.len() / PAGE) as u32;
            self.block_records.resize(self.block_records.len() + PAGE, NONE);
        }
        let slot = self.chunk_pages[chunk] as usize * PAGE + Self::block_slot(x, y, z);
        if self.block_records[slot] == NONE {
            self.block_records[slot] = self.records.len() as u32;
            self.records.push(BlockRecord::new());
        }
        Some(&mut self.records[self.block_records[slot] as usize])
    }

    pub(crate) fn layer0(&self, x: i32, y: i32, z: i32) -> u32 {
        self.record(x, y, z).map_or(0, |r| r.layer0[cell_index(x, y, z)])
    }

    pub(crate) fn layer1(&self, x: i32, y: i32, z: i32) -> u32 {
        self.record(x, y, z).map_or(0, |r| r.layer1[cell_index(x, y, z)])
    }

    /// area_get_collision_cell_resolved: the layer 1 word of a cell, a footprint's resolved to its origin cell.
    pub(crate) fn collision_cell_resolved(&self, x: i32, y: i32, z: i32) -> u32 {
        let v = self.layer1(x, y, z);
        if v & TYPE_MASK != FOOTPRINT {
            return v;
        }
        self.layer1(x - (v & 255) as i32, y - ((v >> 16) & 255) as i32, z - ((v >> 8) & 255) as i32)
    }

    pub(crate) fn set_layer0(&mut self, x: i32, y: i32, z: i32, v: u32) {
        if let Some(r) = self.record_mut(x, y, z) {
            r.layer0[cell_index(x, y, z)] = v;
        }
    }

    pub(crate) fn set_layer1(&mut self, x: i32, y: i32, z: i32, v: u32) {
        if let Some(r) = self.record_mut(x, y, z) {
            r.layer1[cell_index(x, y, z)] = v;
        }
    }

    pub(crate) fn set_object(&mut self, x: i32, y: i32, z: i32, v: u32) {
        if !self.in_range(x, y, z) {
            return;
        }
        if let Some(i) = self.record_id(x, y, z) {
            self.records[i].object[cell_index(x, y, z)] = v;
        }
    }

    pub(crate) fn set_item_set(&mut self, x: i32, y: i32, z: i32, v: u32) {
        if let Some(r) = self.record_mut(x, y, z) {
            let c = cell_index(x, y, z);
            r.item_set[c] = v;
            r.taken[c] = 0;
        }
    }

    /// grid_lookup_cell_value: the item-set word of a cell (the set, and its quarter turns in bits 24 and 25).
    pub(crate) fn item_set(&self, x: i32, y: i32, z: i32) -> u32 {
        self.record(x, y, z).map_or(0, |r| r.item_set[cell_index(x, y, z)])
    }

    /// grid_lookup_cell_mask: which items of a cell's set have been taken out as real items.
    pub(crate) fn taken(&self, x: i32, y: i32, z: i32) -> u32 {
        self.record(x, y, z).map_or(0, |r| r.taken[cell_index(x, y, z)])
    }

    /// grid_set_cell_mask.
    pub(crate) fn set_taken(&mut self, x: i32, y: i32, z: i32, mask: u32) {
        if !self.in_range(x, y, z) {
            return;
        }
        if let Some(i) = self.record_id(x, y, z) {
            self.records[i].taken[cell_index(x, y, z)] = mask;
        }
    }

    /// reset_city_grid_dynamic_flags: every broken wall mended and every item set whole again.
    pub(crate) fn reset_dynamic(&mut self) {
        for r in &mut self.records {
            for c in 0..CELLS {
                if r.layer1[c] & TYPE_MASK == MESH {
                    r.layer1[c] &= 0xff00_ffff;
                }
                r.taken[c] = 0;
            }
        }
    }

    pub(crate) fn create_block(&mut self, x: i32, y: i32, z: i32, v: u32, faces: u8, dims: &dyn BlockDims) {
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

    pub(crate) fn place_with_footprint(&mut self, x: i32, y: i32, z: i32, v: u32, dims: &dyn BlockDims) {
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

    pub(crate) fn clear_cell(&mut self, x: i32, y: i32, z: i32) {
        if !self.in_range(x, y, z) {
            return;
        }

        if let Some(i) = self.record_id(x, y, z) {
            let c = cell_index(x, y, z);
            self.records[i].layer0[c] = 0;
            self.records[i].layer1[c] = 0;
        }
    }

    fn and_layer0(&mut self, x: i32, y: i32, z: i32, mask: u32) {
        if let Some(r) = self.record_mut(x, y, z) {
            r.layer0[cell_index(x, y, z)] &= mask;
        }
    }
}
