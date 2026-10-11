use std::{io::Cursor, path::Path};

use binrw::BinRead;
use subrusta_math::vector::Vector;

use super::LoaderError;

/// A marked point of the item (0 the muzzle, 1 the right hand, 2 the left hand, 4 the second hand's grip).
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct ItmPoint {
    pub kind: i32,
    pub pos: Vector,
}

/// A render vertex and its texture coordinate.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct ItmVertex {
    pub pos: Vector,
    pub uv: [f32; 2],
}

/// A render face: three or four vertex indices.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct ItmFace {
    pub count: u32,
    #[br(count = count)]
    pub indices: Vec<i32>,
}

/// An item model file (data/*.itm, load_item_file): its centre, inertia, marked points and render mesh.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct ItmFile {
    pub version: i32,
    pub center: Vector,
    pub inertia: Vector,
    pub point_count: u32,
    #[br(count = point_count)]
    pub points: Vec<ItmPoint>,
    pub vert_count: u32,
    #[br(count = vert_count)]
    pub verts: Vec<ItmVertex>,
    pub face_count: u32,
    #[br(count = face_count)]
    pub faces: Vec<ItmFace>,
}

impl ItmFile {
    pub fn load(path: &Path) -> Result<Self, LoaderError> {
        let bytes = std::fs::read(path)?;
        Ok(ItmFile::read(&mut Cursor::new(bytes))?)
    }
}
