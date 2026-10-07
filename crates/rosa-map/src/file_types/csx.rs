use std::{io::{Cursor, SeekFrom::Start}, path::Path};

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

    // We dont care about textures, what are we gonna do? Render them?
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
    pub name: [u8; 52],

    #[br(seek_before = Start(offset as u64), restore_position, args(file_type.clone()))]
    pub file: CSXFile
}

impl CSXLookupEntry {
    pub fn name(&self) -> &str {
        let end = self.name.iter().position(|&b| b == 0).unwrap_or(52);
        std::str::from_utf8(&self.name[..end]).unwrap_or("")
    }
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
        let bytes = std::fs::read(map_file)?;

        // Loading this into memory because binrw doesnt do it, so its a lot faster
        let mut cursor = Cursor::new(bytes);

        Ok(CityFileCSX::read(&mut cursor)?)
    }

    pub fn get_building(&self, name: String) -> Option<BuildingFile> {
        for item in &self.lookup_table {
            if item.name() == name {
                return item.file.building.clone()
            }
        }

        println!("Building with name {}", name);

        None
    }
}