use glam::IVec3;
use rosa_map::file_types::sbc::CityFileSBC;

use crate::world::{
    area::{ALL_FACES, AreaGrid, BlockDims, CUBE, MESH, TYPE_MASK},
    ground::Ground,
};

pub struct BlockType {
    pub value: u32,
}

pub fn resolve_block_types(city: &CityFileSBC, block_names: &[String]) -> Vec<BlockType> {
    city.blocktypes
        .iter()
        .map(|bt| {
            let name = bt.name.as_str();
            let mut value = CUBE;

            if !name.is_empty() {
                for (id, n) in block_names.iter().enumerate() {
                    if n == name {
                        value = id as u32 | MESH;
                    }
                }
            }

            BlockType { value }
        })
        .collect()
}

pub fn place_sector_block(grid: &mut AreaGrid, types: &[BlockType], dims: &dyn BlockDims, x: i32, y: i32, z: i32, packed: u32) {
    let r = (packed >> 24) & 3;
    let Some(bt) = types.get((packed & 4095) as usize) else { return };
    if bt.value & TYPE_MASK == CUBE {
        grid.create_block(x, y, z, CUBE, ALL_FACES, dims);
    } else {
        grid.create_block(x, y, z, bt.value + r, ALL_FACES, dims);
    }
}

pub fn instantiate_sectors(city: &CityFileSBC, types: &[BlockType], grid: &mut AreaGrid, ground: &mut Ground, dims: &dyn BlockDims) {
    for sector in &city.sectors {
        let base = sector.pos.0.as_ivec3() * 8;

        for y in 0..8 {
            for z in 0..8 {
                for x in 0..8 {

                    let packed = sector.block_type_indices.get((y * 64 + z * 8 + x) as usize).copied().unwrap_or(0);

                    if packed == 0 {
                        continue;
                    }

                    let p = base + IVec3::new(x, y, z);

                    place_sector_block(grid, types, dims, p.x, p.y, p.z, packed);
                    ground.stamp_roadmap(p.x, p.z, p.y as f32 * 4.0);
                }
            }
        }
    }
}
