use binrw::{BinRead, BinWrite};

pub mod city_sbc;
pub mod building_sbb;
pub mod city_csx;
pub mod item_cmo;
pub mod block_sbl;

#[derive(BinRead, BinWrite, Clone, Default, Debug)]
pub struct Char64 {
    #[br(count = 64)]
    pub string: Vec<u8>
}

impl Char64 {
    pub fn string(&self) -> String {
        String::from_utf8_lossy(&self.string.clone()).replace('\0', "")
    }
}