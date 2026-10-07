use std::io::Write;

use glam::{UVec3, Vec3};
use rosa_map::file_types::{csx::CityFileCSX, sbb::BuildingFile, sbc::CityFileSBC};

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

    // TODO:
    // Refine this to real shapes, this is basically minecraft
    // Slopes, edges, bevels, etc (does alex bevel?)
    pub fn set_block(&mut self, bx: u32, by: u32, bz: u32, value: u32) {
        let (ox, oy, oz) = (bx * 8, by * 8, bz * 8);
        for dy in 0..8 {
            for dz in 0..8 {
                for dx in 0..8 {
                    self.set_cell(ox + dx, oy + dy, oz + dz, value);
                }
            }
        }
    }

    pub fn build(sbc: &CityFileSBC, csx: &CityFileCSX) -> Self {
        let mut grid = Self::new(UVec3::new(64, 8, 64), Vec3::ZERO, 4.0);

        for sector in &sbc.sectors {
            let base = sector.pos.0 * 8;

            for (i, &block_type) in sector.block_type_indices.iter().enumerate() {
                if block_type == 0 { continue; }
                let (lx, ly, lz) = sector_local(i);
                grid.set_cell(base.x + lx, base.y + ly, base.z + lz, block_type);
            }

            //for (i, &iset) in sector.itemset_indices.iter().enumerate() {
            //    if iset == 0 { continue; }
            //    let (lx, ly, lz) = sector_local(i);
            //    let value = resolve_
            //}
        }

        for p in &sbc.buildings {
            let b: BuildingFile = csx.get_building(p.name.as_str().to_string()).unwrap().rotated(p.rot as u8);
            stamp_building(&mut grid, &b, p.pos.0);
        }

        grid
    }
}

fn sector_local(i: usize) -> (u32, u32, u32) {
    let i = i as u32;
    ((i) & 7, (i >> 6) & 7, (i >> 3) & 7)
}

fn stamp_building(grid: &mut AreaGrid, b: &BuildingFile, pos: UVec3) {
    let off = b.offsets.unwrap_or_default().0;

    for ty in 0..b.height {
        for tz in 0..=b.length {
            for tx in 0..=b.width {
                let tile = &b.tiles[BuildingFile::idx(b.width, b.length, tx, tz, ty)];
                if tile.block == 0 { continue; }
                let cx = pos.x as i32 + tx as i32 - off.x as i32;
                let cy = pos.y as i32 + ty as i32 - off.y as i32;
                let cz = pos.z as i32 + tz as i32 - off.z as i32;
                if cx < 0 || cy < 0 || cz < 0 { continue; }

                grid.set_cell(cx as u32, cy as u32, cz as u32, tile.block);

                //if tile.item_set != 0 {
                //    grid.set_cell_interactive(cx, cy, cz, value);
                //}
            }
        }
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

    pub fn set_cell_interactive(&mut self, cx: u32, cy: u32, cz: u32, value: u32) {
        if !self.in_bounds(cx, cy, cz) { return; }

        let ci = self.chunk_index(cx, cy, cz);
        let chunk = self.chunks[ci].get_or_insert_with(|| Box::new(GridChunk::empty()));

        let bi = Self::block_index(cx, cy, cz);
        let block = chunk.blocks[bi].get_or_insert_with(|| Box::new(GridBlock::empty()));

        block.cells[Self::cell_index(cx, cy, cz)].interactive_layer = value;
    }

    pub fn get_cell(&self, cx: u32, cy: u32, cz: u32) -> u32 {
        if !self.in_bounds(cx, cy, cz) { return 0; }
        let Some(chunk) = &self.chunks[self.chunk_index(cx, cy, cz)] else { return 0; };
        let Some(block) = &chunk.blocks[Self::block_index(cx, cy, cz)] else { return 0; };
        block.cells[Self::cell_index(cx, cy, cz)].collision_layer
    }

    // TRANSPARENCY: this is written by claude, why?
    // I need SOME WAY to see if im writing the correct data, and this does it
    // This will be deleted once im confident it works
    pub fn write_heightmap_ppm(&self, path: &str, step: u32) -> std::io::Result<()> {
    let (dx, dz) = (self.dims.x, self.dims.z);
    let max_h = (self.dims.y * 64) as f32;

    // Collect occupied (cx, cz, cy) and track the bounding box so we can crop
    // to the city instead of rendering the whole 4096-wide empty grid.
    let mut cells: Vec<(u32, u32, u32)> = Vec::new();
    let (mut min_x, mut min_z, mut max_x, mut max_z) = (u32::MAX, u32::MAX, 0u32, 0u32);

    for (ci, chunk) in self.chunks.iter().enumerate() {
        let Some(chunk) = chunk else { continue; };
        let ci = ci as u32;
        let (chx, chz, chy) = (ci % dx, (ci / dx) % dz, ci / (dx * dz));
        for (bi, block) in chunk.blocks.iter().enumerate() {
            let Some(block) = block else { continue; };
            let bi = bi as u32;
            let (bx, bz, by) = (bi % 8, (bi / 8) % 8, bi / 64);
            for (li, cell) in block.cells.iter().enumerate() {
                if cell.collision_layer == 0 { continue; }
                let li = li as u32;
                let (lx, lz, ly) = (li % 8, (li / 8) % 8, li / 64);
                let cx = chx * 64 + bx * 8 + lx;
                let cy = chy * 64 + by * 8 + ly;
                let cz = chz * 64 + bz * 8 + lz;
                min_x = min_x.min(cx); max_x = max_x.max(cx);
                min_z = min_z.min(cz); max_z = max_z.max(cz);
                cells.push((cx, cz, cy));
            }
        }
    }

    if cells.is_empty() {
        eprintln!("write_heightmap_ppm: grid is empty, nothing to draw");
        return Ok(());
    }
    //eprintln!("heightmap bbox: x=[{min_x}..{max_x}] z=[{min_z}..{max_z}]");

    let pad = 2 * step;
    let (ox, oz) = (min_x.saturating_sub(pad), min_z.saturating_sub(pad));
    let w = ((max_x + pad - ox) / step + 1) as usize;
    let h = ((max_z + pad - oz) / step + 1) as usize;
    let mut height = vec![0u32; w * h];

    for (cx, cz, cy) in cells {
        let idx = ((cz - oz) / step) as usize * w + ((cx - ox) / step) as usize;
        height[idx] = height[idx].max(cy + 1);
    }

    let mut f = std::fs::File::create(path)?;
    write!(f, "P6\n{w} {h}\n255\n")?;
    let mut buf = Vec::with_capacity(w * h * 3);
    for &hgt in &height {
        let v = if hgt == 0 { 0 } else { (48.0 + (hgt as f32 / max_h) * 207.0) as u8 };
        buf.extend_from_slice(&[v, v, v]);
    }
    f.write_all(&buf)
}
}