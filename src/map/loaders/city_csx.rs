use std::fs::File;
use std::io::{Cursor, Read};

use binrw::BinRead;
use binrw::io::SeekFrom::Start;

use crate::map::loaders::block_sbl::BlockFile;
use crate::map::loaders::building_sbb::BuildingFile;
use crate::map::loaders::Char64;

// NOTE:
// This does **NOT** load textures
// They are skipped because fuck you thats why. (were a server, what are we gonna do? Render?)

#[derive(BinRead, Clone, PartialEq, Debug)]
pub enum CSXFileType {
    #[br(magic = 1482293249u32)] Block,
    #[br(magic = 1482293250u32)] Building, 
    #[br(magic = 1482293252u32)] Texture,
    Unknown
}

#[derive(BinRead, Debug)]
#[br(import(file_type: CSXFileType))]
pub struct CSXFile {
    #[br(if(file_type == CSXFileType::Building))]
    pub building: Option<BuildingFile>,

    #[br(if(file_type == CSXFileType::Block))]
    pub block: Option<BlockFile>
}

#[derive(BinRead, Clone, Default, Debug)]
pub struct CSXTextureHeader {
    pub enabled: u32,
    pub name: Char64,
    pub texture_size: u32,
    pub material_size: u32
}

#[derive(BinRead, Debug)]
pub struct CSXLookupEntry {
    pub file_type: CSXFileType,
    pub offset: u32,
    pub size: u32,
    #[br(count = 52)]
    pub name: Vec<u8>,

    #[br(seek_before = Start(offset as u64), restore_position, args(file_type.clone()))]
    pub file: CSXFile
}

#[derive(BinRead, Debug)]
#[br(little)]
pub struct CityFileCSX {
    pub magic: u32,

    pub lookup_table_offset: u32,
    pub lookup_table_size: u32,

    #[br(seek_before = Start(lookup_table_offset as u64), restore_position, count = lookup_table_size)]
    pub lookup_table: Vec<CSXLookupEntry>
}

impl CityFileCSX {
    pub fn load(city_name: &str) -> Self {
        let path = format!("data/{city_name}/test.csx").to_string();

        let mut data = vec![];
        File::open(path).unwrap().read_to_end(&mut data).unwrap();

        let mut cursor = Cursor::new(data);

        CityFileCSX::read(&mut cursor).unwrap()
    }
}