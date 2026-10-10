use std::{io::Cursor, path::Path};

use binrw::BinRead;
use rosa_math::vector::Vector;

use super::LoaderError;
use crate::Char64;

/// One item of an item set: its type by name, where it sits in the cell and how it is turned.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct SisEntry {
    pub name: Char64,
    pub pos: Vector,
    pub rot: [Vector; 3],
}

/// An item set file (data/itemset/*.sis, load_item_set): items placed together in a level cell.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct ItemSetFile {
    pub version: i32,
    pub count: u32,
    #[br(count = count)]
    pub entries: Vec<SisEntry>,
}

impl ItemSetFile {
    pub fn load(path: &Path) -> Result<Self, LoaderError> {
        let bytes = std::fs::read(path)?;
        Ok(ItemSetFile::read(&mut Cursor::new(bytes))?)
    }
}
