use binrw::{BinRead, BinWrite};

use crate::{map::loaders::Char64, world::vector::IntVector};

#[derive(BinRead, BinWrite)]
pub struct FileBuilding {
    pub name: Char64,
    pub pos: IntVector,
    pub rot: u32
}