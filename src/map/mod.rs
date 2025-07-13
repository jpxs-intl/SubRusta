use std::time::SystemTime;

use crate::{map::{self, loaders::{block_sbl::BlockFile, building_sbb::BuildingFile, city_csx::{CSXFileType, CSXLookupEntry, CityFileCSX}, city_sbc::CityFileSBC}}, world::{block::{FileSectorBlock, FileSectorBlockTypes}, building::FileBuilding}};

pub mod loaders;

pub struct Map {
    pub lookups: Vec<CSXLookupEntry>,
    pub buildings: Vec<FileBuilding>,
    pub blocks: Vec<FileSectorBlockTypes>,
    pub sectors: Vec<FileSectorBlock>
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
        let mut sectors = vec![];

        for data in map_data.buildings {
            buildings.push(data.clone());
        }

        for block in map_data.blocktypes {
            blocks.push(block.clone())
        }

        for sector in &map_data.sectors {
            let x_range = (440..460).contains(&sector.pos.x);
            let y_range = (15..20).contains(&sector.pos.y);
            let z_range = (380..400).contains(&sector.pos.z);

            if x_range || z_range || y_range || (sector.pos.z == 450 && sector.pos.y == 18 &&sector.pos.x == 385) {
                println!("Res!@");
            }

            sectors.push(sector.clone())
        }

        println!("[LOADER] Map loaded and parsed in {}ms", start_time.elapsed().unwrap().as_millis());

        Self {
            lookups: city_lookup.lookup_table,
            buildings,
            blocks,
            sectors
        }
    }
}