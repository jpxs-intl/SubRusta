use std::path::Path;

use glam::{IVec3, Vec3};
use rosa_map::file_types::{csx::CityFileCSX, sbc::CityFileSBC, sbl::BlockFile};

use crate::world::{
    area::{ALL_FACES, AreaGrid, MESH},
    blocks::{BlockTable, StaticDims},
    building::Buildings,
    meshes::{BlockMeshes, PortalGroups},
    city::{instantiate_sectors, resolve_block_types},
    ground::Ground,
    roads::RoadNetwork,
};

pub struct Level {
    pub area: AreaGrid,
    pub tables: BlockTable,
    pub dims: StaticDims,
    pub meshes: BlockMeshes,
    /// The building records (bases, shops, banks, labs) in placement order.
    pub buildings: Vec<crate::world::building::BuildingRecord>,
    /// Each corporation's base.
    pub bases: Vec<crate::world::building::CorporationBase>,
}

fn list_names(dir: &Path, ext: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let n = e.file_name().into_string().ok()?;
                    let stem = n.strip_suffix(ext)?;
                    Some(stem.split('.').next().unwrap_or(stem).to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    v.sort_by_key(|n| n.to_ascii_lowercase());

    v
}

#[allow(clippy::too_many_arguments)]
pub fn build_level(
    city: &CityFileSBC,
    csx: &CityFileCSX,
    ground: &mut Ground,
    roads: &RoadNetwork,
    map_dir: &Path,
    data_dir: &Path,
    map_name: &str,
    game_type: u32,
) -> Level {
    let custom: Vec<(&str, IVec3)> = csx.custom_blocks().map(|(n, b)| (n, b.size.0.as_ivec3())).collect();

    let mut tables = BlockTable::new(&custom.iter().map(|c| c.0).collect::<Vec<&str>>());
    let mut meshes = BlockMeshes::new(&tables, &csx.custom_blocks().map(|(_, b)| b).collect::<Vec<&BlockFile>>());
    let dims = StaticDims::new(&custom.iter().map(|c| c.1).collect::<Vec<_>>());
    let mut area = AreaGrid::new(IVec3::new(64, 8, 64), Vec3::ZERO, 4.0);

    roads.build_blocks(ground, &mut area, &dims);

    let mut item_sets = vec![String::new(); 4];
    item_sets.extend(list_names(&data_dir.join("itemset"), ".sis"));

    let city_iset = |v: u32| -> u32 {
        let name = city.itemset_names.get((v & 1023) as usize).map(|n| {
            let end = n.name.iter().position(|&c| c == 0).unwrap_or(n.name.len());
            String::from_utf8_lossy(&n.name[..end]).into_owned()
        });

        let mut idx = 0;

        if let Some(name) = name {
            for (k, g) in item_sets.iter().enumerate() {
                if *g == name {
                    idx = k as u32;
                }
            }
        }

        idx
    };

    let sector_value = |area_id: u32, x: i32, y: i32, z: i32| -> u32 {
        let mut found = None;

        for s in &city.sectors {
            let p = s.pos.0.as_ivec3();

            if s.area == area_id && p.x == x >> 3 && p.y == y >> 3 && p.z == z >> 3 {
                found = Some(s);
            }
        }

        found.and_then(|s| s.block_indices.get((((y & 7) << 6) | ((z & 7) << 3) | (x & 7)) as usize)).copied().unwrap_or(0)
    };

    for sector in &city.sectors {
        let base = sector.pos.0.as_ivec3() * 8;

        for y in 0..8i32 {
            for z in 0..8i32 {
                for x in 0..8i32 {

                    let i = (y * 64 + z * 8 + x) as usize;
                    let p = base + IVec3::new(x, y, z);
                    let v = sector.block_indices.get(i).copied().unwrap_or(0);

                    if v != 0 && sector.area == 0 && p.x >= 0 && p.y >= 0 && p.z >= 0 {

                        let v = if v == MESH {
                            let open = [(0, -1, 1), (1, 0, 2), (0, 1, 4), (-1, 0, 8)]
                                .iter()
                                .filter(|&&(dx, dz, _)| sector_value(sector.area, p.x + dx, p.y, p.z + dz) == 0)
                                .fold(0u32, |m, &(_, _, bit)| m | bit);
                            open | MESH
                        } else {
                            v
                        };

                        area.create_block(p.x, p.y, p.z, v, ALL_FACES, &dims);
                    }

                    let iv = sector.itemset_indices.get(i).copied().unwrap_or(0);

                    if iv != 0 {
                        area.set_item_set(p.x, p.y, p.z, (iv & (65535 << 16)) | city_iset(iv));
                    }
                }
            }
        }
    }

    let mut block_names = vec![String::new(); tables.total_block_count() as usize];

    for (k, n) in custom.iter().map(|c| c.0).collect::<Vec<&str>>().iter().enumerate() {
        block_names[BlockTable::custom(k) as usize] = n.to_string();
    }

    let types = resolve_block_types(city, &block_names);

    let build_blocks = list_names(&map_dir.join("buildblock"), ".sbl");

    let portals = PortalGroups::load(&map_dir.join("buildblock"), &build_blocks);
    let mut buildings = Buildings::new(csx, build_blocks, item_sets.clone(), game_type);

    for b in &city.buildings {
        buildings.place(&mut area, ground, &mut tables, &dims, b.name.as_str(), b.pos.0.as_ivec3(), b.rot, ground.roundcity());
    }

    for &(id, key) in &buildings.generated {
        meshes.build_generated(id, key, &portals);
    }

    if map_name == "test2" {
        for (x, z) in [(407, 367), (407, 368), (408, 367), (408, 368)] {
            area.set_item_set(x, 17, z, 2);
        }
    }

    instantiate_sectors(city, &types, &mut area, ground, &dims);

    let bases = std::mem::take(&mut buildings.bases);
    let buildings = buildings.records;
    Level { area, tables, dims, meshes, buildings, bases }
}
