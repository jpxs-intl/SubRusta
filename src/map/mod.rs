use std::{collections::HashMap, time::SystemTime};

use dashmap::DashMap;
use rapier3d::prelude::*;

use crate::{
    app_state::AppState,
    map::loaders::{
        block_sbl::{BlockFile, RapierBlock},
        city_csx::{CSXFile, CSXLookupEntry, CityFileCSX},
        city_sbc::CityFileSBC,
    },
    world::{
        block::{Chunk, ChunkBlockTypes},
        building::FileBuilding,
        vector::IntVector,
    },
};

pub mod loaders;

pub struct Map {
    pub lookups: Vec<CSXLookupEntry>,
    pub added_coords: DashMap<(i32, i32, i32), bool>,
    pub buildings: Vec<FileBuilding>,
    pub chunk_blocktypes: Vec<ChunkBlockTypes>,
    pub chunks: Vec<Chunk>,
}

impl Map {
    pub fn load() -> Self {
        println!("[LOADER] Loading the map!");
        let city_name = "test2".to_string();
        let start_time = SystemTime::now();

        println!("[LOADER] Loading {city_name}");
        let city_lookup = CityFileCSX::load(&city_name);
        let map_data = CityFileSBC::load(&city_name);
        println!(
            "[LOADER] {city_name} successfully loaded in {}ms",
            start_time.elapsed().unwrap().as_millis()
        );

        let mut buildings = vec![];
        let mut blocks = vec![];
        let mut chunks = vec![];

        for data in map_data.buildings {
            buildings.push(data.clone());
        }

        for block in map_data.blocktypes {
            blocks.push(block.clone())
        }

        for sector in &map_data.sectors {
            chunks.push(sector.clone())
        }

        println!("[LOADER] Map loaded and parsed in {}ms", start_time.elapsed().unwrap().as_millis());

        Self {
            lookups: city_lookup.lookup_table,
            added_coords: DashMap::new(),
            buildings,
            chunk_blocktypes: blocks,
            chunks,
        }
    }

    pub fn get_sector_by_coordinates(&self, pos: IntVector) -> Option<Chunk> {
        let actual_coords = (pos / 4) / 8;

        for chunk in &self.chunks {
            if chunk.pos.x == actual_coords.x && chunk.pos.z == actual_coords.z {
                return Some(chunk.clone());
            }
        }

        None
    }

    pub fn get_sector_by_block(&self, pos: IntVector) -> Option<Chunk> {
        let actual = pos / 8;

        for chunk in &self.chunks {
            if chunk.pos.x == actual.x && chunk.pos.z == actual.z {
                return Some(chunk.clone());
            }
        }

        None
    }

    pub fn get_blocktype_at_blockpos(&self, pos: IntVector) -> Option<ChunkBlockTypes> {
        if let Some(sector) = self.get_sector_by_block(pos) {
            let block = sector.get_blocktype_at_block(pos % 8) as usize;

            let chunk = self.chunk_blocktypes.get(block & 0x3ff).cloned();

            if let Some(mut chunk) = chunk {
                if block == 65536 {
                    chunk.name.set_string("nblock");
                }

                Some(chunk)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_blocktype_at(&self, pos: IntVector) -> Option<ChunkBlockTypes> {
        if let Some(sector) = self.get_sector_by_coordinates(pos) {
            let block = sector.get_blocktype_at(pos) as usize;

            self.chunk_blocktypes.get(block & 0x3ff).cloned()
        } else {
            None
        }
    }

    pub fn get_blocktype_at_local(&self, chunk: &Chunk, pos: IntVector) -> Option<ChunkBlockTypes> {
        let block = chunk.get_blocktype_at_block(pos) as usize;

        if block == 0 {
            return None;
        }

        let chunk: Option<ChunkBlockTypes> = self.chunk_blocktypes.get(block & 0x3ff).cloned();

        if let Some(mut chunk) = chunk {
            if block == 65536 {
                chunk.name.set_string("nblock");
            }

            Some(chunk)
        } else {
            None
        }
    }

    // TODO: Optimize this.
    pub fn get_file_in_csx(&self, name: &str) -> Option<CSXFile> {
        for file in &self.lookups {
            let fname = String::from_utf8_lossy(&file.name).replace('\0', "");

            if fname == name {
                return Some(file.file.clone());
            }
        }

        None
    }

    pub fn get_block_in_csx(&self, name: &str) -> Option<BlockFile> {
        if let Some(file) = self.get_file_in_csx(name) { file.block } else { None }
    }

    pub fn add_colliders_to_pieces(&self, state: &AppState) {
        let mut created: HashMap<String, RapierBlock> = HashMap::new();

        for chunk in &self.chunks {
            for x in 0..=8 {
                for y in 0..=8 {
                    for z in 0..=8 {
                        let proper = self.get_blocktype_at_local(chunk, IntVector { x, y, z });

                        if let Some(proper) = proper {
                            let block = self.get_block_in_csx(&proper.name.string());

                            if let Some(block) = block {
                                if let Some(existing) = created.get(&proper.name.string()) {
                                    // Again, cube gen.
                                    {
                                        let cube_data = &existing.0;
                                        let mesh = ColliderBuilder::trimesh(cube_data.0.clone(), cube_data.1.clone());

                                        if let Ok(mesh) = mesh {
                                            state.physics.insert_collider(
                                                mesh.translation(vector![
                                                    (chunk.pos.x as f32 * 8.0 + x as f32) * 4.0,
                                                    (chunk.pos.y as f32 * 8.0 + y as f32) * 4.0,
                                                    (chunk.pos.z as f32 * 8.0 + z as f32) * 4.0
                                                ])
                                                .build(),
                                            );
                                        }
                                    }

                                    {
                                        let surface_data = &existing.1;
                                        let mesh = ColliderBuilder::convex_hull(surface_data);

                                        if let Some(mesh) = mesh {
                                            state.physics.insert_collider(
                                                mesh.translation(vector![
                                                    (chunk.pos.x as f32 * 8.0 + x as f32) * 4.0,
                                                    (chunk.pos.y as f32 * 8.0 + y as f32) * 4.0,
                                                    (chunk.pos.z as f32 * 8.0 + z as f32) * 4.0
                                                ])
                                                .build()
                                            );
                                        }
                                    }
                                } else {
                                    let rapier = block.cubes_to_rapier();

                                    created.insert(proper.name.string(), rapier.clone());

                                    // This part is for generating the cubes, (blocks as Alex calls em)
                                    {
                                        let cube_data = rapier.0;
                                        let mesh = ColliderBuilder::trimesh(cube_data.0, cube_data.1);
                                        if let Ok(mesh) = mesh {
                                            state.physics.insert_collider(
                                                mesh.translation(vector![
                                                    (chunk.pos.x as f32 * 8.0 + x as f32) * 4.0,
                                                    (chunk.pos.y as f32 * 8.0 + y as f32) * 4.0,
                                                    (chunk.pos.z as f32 * 8.0 + z as f32) * 4.0
                                                ])
                                                .build(),
                                            );
                                        }
                                    }

                                    {
                                        let surface_data = rapier.1;
                                        let mesh = ColliderBuilder::convex_hull(&surface_data);

                                        if let Some(mesh) = mesh {
                                            state.physics.insert_collider(
                                                mesh.translation(vector![
                                                    (chunk.pos.x as f32 * 8.0 + x as f32) * 4.0,
                                                    (chunk.pos.y as f32 * 8.0 + y as f32) * 4.0,
                                                    (chunk.pos.z as f32 * 8.0 + z as f32) * 4.0
                                                ])
                                                .build(),
                                            );
                                        }
                                    }
                                }
                            } else if proper.name.string() == "nblock" {
                                // TODO: Diagnose this shit, I have NO idea why its like this.
                                // Its just a empty cube :shrug:
                                let cube = ColliderBuilder::cuboid(2.0, 2.0, 2.0)
                                    .translation(vector![
                                        (chunk.pos.x as f32 * 8.0 + x as f32) * 4.0 + 2.0,
                                        (chunk.pos.y as f32 * 8.0 + y as f32) * 4.0 + 2.0,
                                        (chunk.pos.z as f32 * 8.0 + z as f32) * 4.0 + 2.0
                                    ])
                                    .build();

                                state.physics.insert_collider(cube);
                            }
                        }
                    }
                }
            }
        }
    }
}
