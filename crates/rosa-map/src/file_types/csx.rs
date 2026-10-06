use std::{fs::File, io::SeekFrom::Start, path::Path};

use binrw::BinRead;

use crate::{Char64, file_types::{LoaderError, sbb::BuildingFile, sbl::BlockFile}};

#[derive(BinRead, Clone, PartialEq, Debug)]
pub enum CSXFileType {
    #[br(magic = 1482293249u32)] Block,
    #[br(magic = 1482293250u32)] Building,
    #[br(magic = 1482293252u32)] Texture,
    Unknown
}

#[derive(BinRead, Debug, Clone)]
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

#[derive(BinRead, Debug, Clone)]
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
    pub fn load(path: &Path) -> Result<Self, LoaderError> {
        let map_file = path.join("test.csx");

        let mut file = File::open(map_file).unwrap();

        Ok(CityFileCSX::read(&mut file)?)
    }
}