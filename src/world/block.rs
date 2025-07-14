use binrw::{BinRead, BinWrite};

use crate::{map::loaders::Char64, world::vector::IntVector};

#[derive(BinRead, BinWrite, Clone, Debug)]
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

impl Chunk {
    pub fn get_blocktype_at_local(&self, pos: IntVector) -> u32 {
        let index = 64 * pos.y + 8 * pos.z + pos.x;

        *self.block_type_indices.get(index as usize).unwrap_or(&0)
    }
    
    pub fn get_blocktype_at(&self, pos: IntVector) -> u32 {
        let index = 64 * (pos.y % 8) + 8 * (pos.z % 8) + (pos.x % 8);

        *self.block_type_indices.get(index as usize).unwrap_or(&0)
    }
}

#[derive(BinRead, BinWrite, Clone, Debug)]
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