use std::fs::File;

use binrw::{binrw, BinRead, BinWrite};

use crate::world::{block::{FileSectorBlock, FileSectorBlockTypes}, building::FileBuilding, street::FileStreet, vector::{IntVector, Vector}};

#[derive(BinRead, BinWrite)]
pub struct ItemSet {
    #[br(count = 64)]
    pub name: Vec<u8>
}

#[binrw]
#[brw(little)]
pub struct CityFile {
    pub version: u32,

    #[br(if(version >= 10))]
    pub num_itemsets: u32,
    #[br(if(version >= 10), count = num_itemsets)]
    pub itemset_names: Vec<ItemSet>,

    pub num_intersections: u32,
    #[br(count = num_intersections)]
    pub intersections: Vec<IntVector>,

    pub num_streets: u32,
    #[br(count = num_streets)]
    pub streets: Vec<FileStreet>,

    pub num_buildings: u32,
    #[br(count = num_buildings)]
    pub buildings: Vec<FileBuilding>,

    #[br(if(version == 11 || version >= 12))]
    pub num_blocktypes: u32,
    #[br(if(version == 11 || version >= 12), args { count: num_blocktypes as usize, inner: (version,) })]
    pub blocktypes: Vec<FileSectorBlockTypes>,

    #[br(if(version >= 8))]
    pub num_sectors: u32,
    #[br(if(version >= 8), args { count: num_sectors as usize, inner: (version,) })]
    pub sectors: Vec<FileSectorBlock>,

    #[br(if(version >= 9))]
    pub num_waypoints: u32,
    #[br(if(version >= 9), count = num_waypoints)]
    pub waypoints: Vec<Vector>
}

impl CityFile {
    pub fn load(map_name: &str) -> Self {
        let path = format!("data/{map_name}/city2.sbc").to_string();

        let mut file = File::open(path).unwrap();

        CityFile::read(&mut file).unwrap()
    }
}