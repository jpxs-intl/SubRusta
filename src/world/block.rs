use binrw::{BinRead, BinWrite};

use crate::{map::loaders::Char64, world::vector::IntVector};

#[derive(BinRead, BinWrite)]
#[br(import(version: u32))]
pub struct FileSectorBlock {
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

#[derive(BinRead, BinWrite)]
#[br(import(version: u32))]
pub struct FileSectorBlockTypes {
    pub name: Char64,

    #[br(if(version >= 12))]
    pub count: u32,
    #[br(if(version >= 12), count = count * 8)]
    pub texture_names_12: Vec<Char64>,

    #[br(if(version == 11), count = 8)]
    pub texture_names_11: Vec<Char64>
}