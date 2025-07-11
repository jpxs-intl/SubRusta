use std::{fs::File, io::{Cursor, Read}};

use binrw::BinRead;

use crate::world::vector::Vector;

#[derive(BinRead)]
#[br(import(version: u32))]
pub struct ItemModelVertex {
    pub pos: Vector,
    #[br(if(version >= 3))]
    pub vertex1: f32,
    #[br(if(version >= 3))]
    pub vertex2: f32,
    pub padding: u32
}

#[derive(BinRead)]
#[br(import(version: u32))]
pub struct ItemModelFace {
    pub num_vertices: u32,
    #[br(count = num_vertices)]
    pub vertex_ids: Vec<u32>,

    #[br(if(version > 1))]
    _unk3: u32,
    #[br(if(version < 1))]
    _unk4: u64
}

#[derive(BinRead)]
#[br(little)]
pub struct ItemModel {
    pub magic: u32,
    pub version: u32,

    pub vertex_count: u32,
    #[br(args { count: vertex_count as usize, inner: (version,) })]
    pub vertices: Vec<ItemModelVertex>,

    pub face_count: u32,
    #[br(args { count: face_count as usize, inner: (version,) })]
    pub faces: Vec<ItemModelFace>
}

impl ItemModel {
    pub fn load(name: &str) -> Option<Self> {
        let file_name = format!("data/model/{name}.cmo").to_string();

        let mut data = vec![];
        let _ = File::open(file_name).ok()?.read_to_end(&mut data);

        ItemModel::read(&mut Cursor::new(&mut data)).ok()
    }
}