use glam::{UVec3, Vec3};

pub struct AreaGrid {
    origin: Vec3,
    block_size: f32,
    dims: UVec3,
    chunks: Vec<Option<Box<GridChunk>>>
}
struct GridChunk { blocks: [Option<Box<GridBlock>>; 512] }
struct GridBlock { cells: [GridCell; 512] }
#[derive(Clone, Default)]
struct GridCell { collision_layer: u32, interactive_layer: u32 }
impl GridCell { const EMPTY: GridCell = GridCell { collision_layer: 0, interactive_layer: 0 }; }

impl GridChunk {
    pub fn empty() -> Self {
        Self { blocks: [const { None }; 512] }
    }
}

impl GridBlock {
    pub fn empty() -> Self {
        Self { cells: [ GridCell::EMPTY; 512 ] }
    }
}

impl AreaGrid {
    pub fn new(dims: UVec3, origin: Vec3, block_size: f32) -> Self {
        let n = (dims.x * dims.y * dims.z) as usize;
        Self { origin, block_size, dims, chunks: (0..n).map(|_| None).collect() }
    }
}

impl AreaGrid {
    #[inline] fn chunk_index(&self, cx: u32, cy: u32, cz: u32) -> usize {
        (((cy >> 6) * self.dims.z + (cz >> 6)) * self.dims.x + (cx >> 6)) as usize
    }
    #[inline] fn block_index(cx: u32, cy: u32, cz: u32) -> usize {
        (((cy >> 3) & 7) * 64 + ((cz >> 3) & 7) * 8 + ((cx >> 3) & 7)) as usize
    }
    #[inline] fn cell_index(cx: u32, cy: u32, cz: u32) -> usize {
        ((cy & 7) * 64 + (cz & 7) * 8 + (cx & 7)) as usize
    }
    #[inline] fn in_bounds(&self, cx: u32, cy: u32, cz: u32) -> bool {
        cx >> 6 < self.dims.x && cy >> 6 < self.dims.y && cz >> 6 < self.dims.z
    }
}

impl AreaGrid {
    pub fn set_cell(&mut self, cx: u32, cy: u32, cz: u32, value: u32) {
        if !self.in_bounds(cx, cy, cz) { return; }
        let ci = self.chunk_index(cx, cy, cz);
        let chunk = self.chunks[ci].get_or_insert_with(|| Box::new(GridChunk::empty()));

        let bi = Self::block_index(cx, cy, cz);
        let block = chunk.blocks[bi].get_or_insert_with(|| Box::new(GridBlock::empty()));

        block.cells[Self::cell_index(cx, cy, cz)].collision_layer = value;
    }

    pub fn get_cell(&self, cx: u32, cy: u32, cz: u32) -> u32 {
        if !self.in_bounds(cx, cy, cz) { return 0; }
        let Some(chunk) = &self.chunks[self.chunk_index(cx, cy, cz)] else { return 0; };
        let Some(block) = &chunk.blocks[Self::block_index(cx, cy, cz)] else { return 0; };
        block.cells[Self::cell_index(cx, cy, cz)].collision_layer
    }
}