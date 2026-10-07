use glam::{UVec3, Vec3};
use rosa_map::file_types::{csx::CityFileCSX, sbb::BuildingFile, sbc::CityFileSBC};

use crate::world::{ground::Ground, roads::RoadNetwork};

#[allow(unused)]
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

    pub fn build(sbc: &CityFileSBC, csx: &CityFileCSX, ground: &mut Ground, roads: &RoadNetwork) -> Self {
        let mut grid = Self::new(UVec3::new(64, 8, 64), Vec3::ZERO, 4.0);

        roads.stamp(ground, &mut grid);

        for p in &sbc.buildings {
            let b: BuildingFile = csx.get_building_rotated(p.name.as_str().to_string(), p.rot as u8).unwrap();
            stamp_building(ground, &mut grid, &b, p.pos.0);
        }

        for sector in &sbc.sectors {
            let base = sector.pos.0 * 8;

            for (i, &block_type) in sector.block_type_indices.iter().enumerate() {
                if block_type == 0 { continue; }
                let (lx, ly, lz) = sector_local(i);
                let (x, y, z) = (base.x + lx, base.y + ly, base.z + lz);
                grid.set_cell(x, y, z, block_type);
                ground.stamp_roadmap(x as i32, z as i32, y as f32 * 4.0);
            }

            //for (i, &iset) in sector.itemset_indices.iter().enumerate() {
            //    if iset == 0 { continue; }
            //    let (lx, ly, lz) = sector_local(i);
            //    let value = resolve_
            //}
        }

        grid
    }
}

fn sector_local(i: usize) -> (u32, u32, u32) {
    let i = i as u32;
    ((i) & 7, (i >> 6) & 7, (i >> 3) & 7)
}

fn stamp_building(ground: &mut Ground, grid: &mut AreaGrid, b: &BuildingFile, pos: UVec3) {
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

                if ty == off.y {
                    ground.stamp_roadmap(cx, cz, cy as f32 * 4.0);
                }
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
}

impl AreaGrid {
    /// Two separate layers:
    ///   terrain (grass) height  → red (low) … blue (high)   [from `base`, the generate_grass field]
    ///   roads/buildings height  → grayscale (dark=low, white=high), drawn on top  [from the block grid]
    /// Terrain and structures are queried from different sources, so they're independent.
    pub fn write_world_ppm(&self, ground: &Ground, path: &str, step: u32) -> std::io::Result<()> {
        use std::io::Write;
        let (dx, dz) = (self.dims.x, self.dims.z);

        // --- pass 1: block tops (world Y of the tallest block per cell) + bbox ---
        let (mut minx, mut minz, mut maxx, mut maxz) = (u32::MAX, u32::MAX, 0u32, 0u32);
        let mut cells: Vec<(u32, u32, u32)> = Vec::new(); // (cx, cz, top_cell = cy+1)
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
                    minx = minx.min(cx); maxx = maxx.max(cx);
                    minz = minz.min(cz); maxz = maxz.max(cz);
                    cells.push((cx, cz, cy + 1));
                }
            }
        }
        if cells.is_empty() { eprintln!("write_world_ppm: empty grid"); return Ok(()); }

        let margin = 24;
        let ox = minx.saturating_sub(margin);
        let oz = minz.saturating_sub(margin);
        let ex = (maxx + margin).min(dx * 64 - 1);
        let ez = (maxz + margin).min(dz * 64 - 1);
        let w = ((ex - ox) / step + 1) as usize;
        let h = ((ez - oz) / step + 1) as usize;

        // per-pixel tallest block top (0 = no block there)
        let mut block_top = vec![0u32; w * h];
        for (cx, cz, top) in &cells {
            if *cx < ox || *cz < oz || *cx > ex || *cz > ez { continue; }
            let idx = ((cz - oz) / step) as usize * w + ((cx - ox) / step) as usize;
            if *top > block_top[idx] { block_top[idx] = *top; }
        }

        // --- separate height ranges for the two layers ---
        let (mut tmin, mut tmax) = (f32::MAX, f32::MIN);   // terrain
        for pz in 0..h { for px in 0..w {
            let (cx, cz) = (ox + px as u32 * step, oz + pz as u32 * step);
            if let Some(ty) = ground.height_at(cx as f32 * 4.0 + 2.0, cz as f32 * 4.0 + 2.0) {
                tmin = tmin.min(ty); tmax = tmax.max(ty);
            }
        }}
        if tmax <= tmin { tmax = tmin + 1.0; }

        let (mut bmin, mut bmax) = (f32::MAX, f32::MIN);   // block tops (world units)
        for &t in block_top.iter().filter(|&&t| t > 0) {
            let bh = t as f32 * 4.0;
            bmin = bmin.min(bh); bmax = bmax.max(bh);
        }
        if bmax <= bmin { bmax = bmin + 1.0; }

        // --- compose: terrain red→blue background, then structures grayscale on top ---
        let mut buf = vec![0u8; w * h * 3];
        for pz in 0..h { for px in 0..w {
            let i = (pz * w + px) * 3;
            let (cx, cz) = (ox + px as u32 * step, oz + pz as u32 * step);

            // layer 1: collision terrain height → red (low) … blue (high); black = masked / none
            if let Some(ty) = ground.height_at(cx as f32 * 4.0 + 2.0, cz as f32 * 4.0 + 2.0) {
                let t = ((ty - tmin) / (tmax - tmin)).clamp(0.0, 1.0);
                buf[i]     = ((1.0 - t) * 255.0) as u8;
                buf[i + 1] = 0;
                buf[i + 2] = (t * 255.0) as u8;
            }

            // layer 2: roads/buildings height → grayscale, drawn over terrain
            let top = block_top[pz * w + px];
            if top > 0 {
                let s = ((top as f32 * 4.0 - bmin) / (bmax - bmin)).clamp(0.0, 1.0);
                let v = (60.0 + s * 195.0) as u8;
                buf[i] = v; buf[i + 1] = v; buf[i + 2] = v;
            }
        }}

        let mut f = std::fs::File::create(path)?;
        write!(f, "P6\n{w} {h}\n255\n")?;
        f.write_all(&buf)
    }
}

pub fn terrain_height_world(base: &[f32], wx: f32, wz: f32) -> f32 {
    let fx = ((wx + 4096.0) / 8.0).clamp(0.0, (2049 - 2) as f32);
    let fz = ((wz + 4096.0) / 8.0).clamp(0.0, (2049 - 2) as f32);
    let (x0, z0) = (fx.floor() as usize, fz.floor() as usize);
    let (tx, tz) = (fx - x0 as f32, fz - z0 as f32);

    let h00 = base[z0 * 2049 + x0];
    let h10 = base[z0 * 2049 + x0 + 1];
    let h01 = base[(z0 + 1) * 2049 + x0];
    let h11 = base[(z0 + 1) * 2049 + x0 + 1];

    (1.0 - tx) * (h00 * (1.0 - tz) + h01 * tz)
        + tx * (tz * h11 + (1.0 - tz) * h10)
}