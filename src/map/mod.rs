use std::time::SystemTime;

use dashmap::DashMap;
use rapier3d::parry::utils::hashmap::HashMap;

use crate::{map::loaders::{block_sbl::BlockFile, city_csx::{CSXFile, CSXLookupEntry, CityFileCSX}, city_sbc::CityFileSBC}, world::{block::{Chunk, ChunkBlockTypes}, building::FileBuilding, vector::IntVector}};

pub mod loaders;

pub struct Map {
    pub lookups: Vec<CSXLookupEntry>,
    pub added_coords: DashMap<(i32, i32, i32), bool>,
    pub buildings: Vec<FileBuilding>,
    pub chunk_blocktypes: Vec<ChunkBlockTypes>,
    pub chunks: Vec<Chunk>
}

impl Map {
    pub fn load() -> Self {
        println!("[LOADER] Loading the map!");
        let city_name = "test2".to_string();
        let start_time = SystemTime::now();

        println!("[LOADER] Loading {city_name}");
        let city_lookup = CityFileCSX::load(&city_name);
        let map_data = CityFileSBC::load(&city_name);
        println!("[LOADER] {city_name} successfully loaded in {}ms", start_time.elapsed().unwrap().as_millis());

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
            chunks
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

            self.chunk_blocktypes.get(block & 0x3ff).cloned()
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
        if let Some(sector) = self.get_sector_by_coordinates(chunk.pos * 8) {
            let block = sector.get_blocktype_at_block(pos) as usize;

            self.chunk_blocktypes.get(block & 0x3ff).cloned()
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
        if let Some(file) = self.get_file_in_csx(name) {
            file.block
        } else {
            None
        }
    }
}