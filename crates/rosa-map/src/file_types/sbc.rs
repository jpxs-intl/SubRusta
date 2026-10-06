use std::{fs::File, path::Path};

use binrw::{BinRead, BinWrite, binread};
use rosa_math::vector::{IntVector, Vector};

use crate::{Char64, file_types::LoaderError};

#[derive(BinRead, Clone, Debug)]
#[br(import(version: u32))]
pub struct Chunk {
    pub area: u32,
    pub pos: IntVector,
    #[br(if(version >= 11), count = 512)]
    pub block_type_indices: Vec<u32>,

    #[br(if(version < 11), count = 512)]
    pub block_indices: Vec<u32>,
    #[br(if(version < 11), count = 4096)]
    pub texture_indices: Vec<u16>,

    #[br(if(version >= 10), count = 512)]
    pub itemset_indices: Vec<u32>
}

#[derive(BinRead, Clone, Debug)]
#[br(import(version: u32))]
pub struct ChunkBlockTypes {
    pub name: Char64,

    #[br(if(version >= 12))]
    pub count: u32,
    #[br(if(version >= 12), count = count * 8)]
    pub texture_names_12: Vec<Char64>,

    #[br(if(version == 11), count = 8)]
    pub texture_names_11: Vec<Char64>
}

#[derive(BinRead, Clone)]
pub struct FileBuilding {
    pub name: Char64,
    pub pos: IntVector,
    pub rot: u32
}

#[derive(BinRead, Clone)]
pub struct FileStreet {
    pub intersection_indices: [u32; 2],
    pub direction: u32,
    pub left_lane: u32,
    pub right_lane: u32,
    #[br(count = 32)]
    pub name: Vec<u8>
}

#[derive(BinRead, BinWrite)]
pub struct ItemSet {
    #[br(count = 64)]
    pub name: Vec<u8>
}

#[binread]
#[brw(little)]
pub struct CityFileSBC {
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
    pub blocktypes: Vec<ChunkBlockTypes>,

    #[br(if(version >= 8))]
    pub num_sectors: u32,
    #[br(if(version >= 8), args { count: num_sectors as usize, inner: (version,) })]
    pub sectors: Vec<Chunk>,

    #[br(if(version >= 9))]
    pub num_waypoints: u32,
    #[br(if(version >= 9), count = num_waypoints)]
    pub waypoints: Vec<Vector>
}

impl CityFileSBC {
    pub fn load(path: &Path) -> Result<Self, LoaderError> {
        let map_file = path.join("city2.sbc");

        let mut file = File::open(map_file).unwrap();

        Ok(CityFileSBC::read(&mut file)?)
    }
}