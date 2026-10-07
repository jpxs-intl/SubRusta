use std::{io::{Cursor, SeekFrom::Start}, path::Path};

use binrw::BinRead;

use crate::{Char64, file_types::{LoaderError, sbb::{BuildingFile, SpecialBlock}, sbl::BlockFile}};

/// area_create_blocks: builtin_block_count; builtin specials start here.
pub const BUILTIN_BLOCK_COUNT: u32 = 0x284;
/// area_create_blocks: first global id of the CSX (custom) blocks.
pub const CUSTOM_BLOCK_BASE: u32 = 0x2CC;

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

    /// area_create_blocks: the CSX's Block entries, in lookup-table order. Entry k owns the global
    /// block ids CUSTOM_BLOCK_BASE + 4k .. + 3, one per orientation.
    pub fn custom_blocks(&self) -> impl Iterator<Item = (&str, &BlockFile)> {
        self.lookup_table.iter()
            .filter(|e| e.file_type == CSXFileType::Block)
            .filter_map(|e| Some((e.name(), e.file.block.as_ref()?)))
    }

    /// block_dimensions depth (+0xc8) of a global block id. Custom blocks: orientation 0 is the
    /// file's depth, and copy_block_data swaps width/depth for each further orientation.
    /// Builtin ids (< CUSTOM_BLOCK_BASE) a building can reach here are 0..=3 (unresolved
    /// specials), which are 1.
    pub fn block_footprint(&self, id: u32) -> i32 {
        let Some(k) = id.checked_sub(CUSTOM_BLOCK_BASE) else { return 1 };
        match self.custom_blocks().nth((k / 4) as usize) {
            Some((_, b)) if k % 2 == 0 => b.size.0.z as i32,
            Some((_, b)) => b.size.0.x as i32,
            None => 1,
        }
    }

    /// Resolve a building's special block `0x8R00_IIII` the way building_setup_build does:
    /// global id = (index of the block named special_blocks[I] among the custom blocks, last
    /// match wins, 0 if none) + R.
    pub fn special_block(&self, building: &BuildingFile, raw: u32) -> SpecialBlock {
        let name = building.special_blocks.get((raw & 0x3FF) as usize).map(|n| n.as_str());
        let base = name
            .and_then(|n| self.custom_blocks().enumerate().filter(|(_, (bn, _))| *bn == n).last())
            .map_or(0, |(k, _)| CUSTOM_BLOCK_BASE + 4 * k as u32);
        let id = base + ((raw >> 24) & 3);
        let custom_end = CUSTOM_BLOCK_BASE + 4 * self.custom_blocks().count() as u32;
        SpecialBlock {
            footprint: self.block_footprint(id),
            rotates: (BUILTIN_BLOCK_COUNT..custom_end).contains(&id),
        }
    }

    /// The building as load_map instantiates it after `quarter_turns` rotations.
    pub fn get_building_rotated(&self, name: String, quarter_turns: u8) -> Option<BuildingFile> {
        Some(self.get_building(name)?.rotated(quarter_turns, &|b, raw| self.special_block(b, raw)))
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