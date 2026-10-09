use std::{
    io::{Cursor, Read},
    path::Path,
};

use binrw::BinRead;
use rosa_math::vector::Vector;

use super::{LoaderError, sbv::SbvPart};

const MESH_VERSION: i32 = 2;
const PARTS_VERSION: i32 = 1;

/// A vertex of a test shape mesh and the number stored beside it.
#[derive(BinRead, Debug, Clone, Copy)]
#[brw(little)]
pub struct TstVertex {
    pub pos: Vector,
    pub id: i32,
}

/// A corner of a test shape face: a vertex and its texture coordinates.
#[derive(BinRead, Debug, Clone, Copy)]
#[brw(little)]
pub struct TstCorner {
    pub vertex: i32,
    pub uv: [f32; 2],
}

/// A face of up to four corners.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct TstFace {
    pub count: u32,
    #[br(count = count)]
    pub corners: Vec<TstCorner>,
    // TODO: name once its readers are ported (face +0x34)
    pub unk_34: i32,
}

/// A mesh block (tst_read_heli, version 2).
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct TstMesh {
    // TODO: name once its readers are ported
    pub unk_04: i32,
    pub vertex_count: u32,
    #[br(count = vertex_count)]
    pub vertices: Vec<TstVertex>,
    pub face_count: u32,
    #[br(count = face_count)]
    pub faces: Vec<TstFace>,
}

/// A parts block (tst_read_van, version 1): groups of up to four points.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct TstParts {
    pub part_count: u32,
    #[br(count = part_count)]
    pub parts: Vec<SbvPart>,
}

/// A test shape file (data/<name>.tst, read by load_tst): a mesh block, a parts block and, for Van2, a second mesh
/// block. Each block starts with its version and a block of the wrong version is left unread after it.
#[derive(Debug, Clone, Default)]
pub struct TstFile {
    pub mesh: Option<TstMesh>,
    pub parts: Option<TstParts>,
    pub chassis: Option<TstMesh>,
}

fn block<T: for<'a> BinRead<Args<'a> = ()>>(r: &mut Cursor<Vec<u8>>, version: i32) -> Option<T> {
    let mut v = [0u8; 4];
    r.read_exact(&mut v).ok()?;
    (i32::from_le_bytes(v) == version).then(|| T::read_le(r).ok()).flatten()
}

impl TstFile {
    /// The blocks load_tst reads, the chassis mesh only when `chassis` is set.
    pub fn load(path: &Path, chassis: bool) -> Result<Self, LoaderError> {
        let mut r = Cursor::new(std::fs::read(path)?);
        let mesh = block(&mut r, MESH_VERSION);
        let parts = block(&mut r, PARTS_VERSION);
        let chassis = if chassis { block(&mut r, MESH_VERSION) } else { None };
        Ok(TstFile { mesh, parts, chassis })
    }
}
