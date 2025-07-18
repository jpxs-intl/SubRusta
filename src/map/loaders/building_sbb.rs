use std::{fs::File, io::{Cursor, Read}};

use binrw::BinRead;

use crate::{
    map::loaders::Char64,
    world::vector::{IntVector, Vector},
};

#[derive(BinRead, Debug, Clone)]
pub struct BuildingFileTile {
    pub block: u32,
    pub interior_block: u32,
    pub build_block: u32,
    pub edge_x: u32,
    pub edge_z: u32,
    pub floor: u32,

    #[br(count = 8)]
    pub texture_indices: Vec<u16>,
    #[br(count = 8)]
    pub interior_texture_indices: Vec<u16>,

    pub item_set: u32,
}

#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct BuildingFileWaypoint {
    pub waypoint: Vector,
    pub zero: u32,
}

#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct BuildingFileBot {
    pub waypoint_amount: u32,
    #[br(count = waypoint_amount)]
    pub waypoints: Vec<BuildingFileWaypoint>,
}

#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct BuildingFile {
    pub version: u32,
    pub name: Char64,
    pub width: u32,
    pub length: u32,
    pub height: u32,
    #[br(if(version > 12))]
    pub offsets: Option<IntVector>,

    pub texture_count: u32,
    #[br(count = texture_count)]
    pub texture_names: Vec<Char64>,

    pub special_block_count: u32,
    #[br(count = special_block_count)]
    pub special_blocks: Vec<Char64>,

    pub build_block_count: u32,
    #[br(count = build_block_count)]
    pub build_blocks: Vec<Char64>,

    pub item_set_count: u32,
    #[br(count = item_set_count)]
    pub item_sets: Vec<Char64>,

    #[br(if(height > 0), count = height * (length + 1) * (width + 1))]
    pub tiles: Vec<BuildingFileTile>,

    #[br(if(version > 13))]
    pub waypoints_enabled: u32,
    #[br(if(version > 13))]
    pub bot_count: u32,
    #[br(if(version > 13), count = bot_count)]
    pub bots: Vec<BuildingFileBot>,
}

impl BuildingFile {
    pub fn load(city: &str, building_name: String) -> Option<Self> {
        let path = format!("data/{city}/building/{building_name}.sbb").to_string();

        let mut data = vec![];
        let _ = File::open(path).ok()?.read_to_end(&mut data);

        BuildingFile::read(&mut Cursor::new(data)).ok()
    }
}
