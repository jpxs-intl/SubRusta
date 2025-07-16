use std::fmt::Debug;

use binrw::{BinRead, BinWrite};

use crate::connection::packets::utils::limited_string;

pub mod city_sbc;
pub mod building_sbb;
pub mod city_csx;
pub mod item_cmo;
pub mod block_sbl;

#[derive(BinRead, BinWrite, Clone, Default)]
pub struct Char64 {
    #[br(count = 64)]
    pub string: Vec<u8>
}

impl Char64 {
    pub fn string(&self) -> String {
        String::from_utf8_lossy(&self.string.clone()).replace('\0', "")
    }

    pub fn set_string(&mut self, string: &str) {
        let name = limited_string(string, 64);

        self.string = name;
    }
}

impl Debug for Char64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Char64").field("string", &self.string()).finish()
    }
}