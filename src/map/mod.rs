use std::time::SystemTime;

use crate::map::loaders::{block_sbl::BlockFile, building_sbb::BuildingFile, city_csx::{CSXFileType, CityFileCSX}};

pub mod loaders;

pub struct Map {
    pub city_file: CityFileCSX,
    pub buildings: Vec<BuildingFile>,
    pub blocks: Vec<BlockFile>
}

impl Map {
    pub fn load() -> Self {
        println!("[LOADER] Loading the map!");
        let city_name = "test2".to_string();
        let start_time = SystemTime::now();

        println!("[LOADER] Loading {city_name}.csx");
        let city_file = CityFileCSX::load(&city_name);
        println!("[LOADER] {city_name}.csx successfully loaded in {}ms", start_time.elapsed().unwrap().as_millis());

        let mut buildings = vec![];
        let mut blocks = vec![];

        for data in &city_file.lookup_table {
            if let CSXFileType::Block = data.file_type {
                blocks.push(data.file.block.as_ref().unwrap().clone())
            } else if let CSXFileType::Building = data.file_type {
                buildings.push(data.file.building.as_ref().unwrap().clone())
            }
        }

        println!("[LOADER] Map loaded and parsed in {}ms", start_time.elapsed().unwrap().as_millis());

        Self {
            city_file,
            buildings,
            blocks
        }
    }
}