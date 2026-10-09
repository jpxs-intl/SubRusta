use std::{io::Cursor, path::Path};

use binrw::BinRead;
use rosa_math::vector::Vector;

use super::LoaderError;

/// A chassis point: its position and a weight.
#[derive(BinRead, Debug, Clone, Copy)]
#[brw(little)]
pub struct SbvNode {
    pub pos: Vector,
    pub weight: f32,
}

/// A spring between two chassis points.
#[derive(BinRead, Debug, Clone, Copy)]
#[brw(little)]
pub struct SbvEdge {
    pub kind: i32,
    pub a: i32,
    pub b: i32,
}

/// A face of the body over chassis points.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct SbvFace {
    pub count: u32,
    #[br(count = count)]
    pub indices: Vec<i32>,
}

/// A vertex of a subshape and (version 2 on) its weights over the subshape's four chassis points.
#[derive(BinRead, Debug, Clone, Copy)]
#[brw(little)]
#[br(import(version: u32))]
pub struct SbvSubshapeNode {
    pub id: i32,
    #[br(if(version > 1, [0.0; 4]))]
    pub weights: [f32; 4],
}

/// Render vertices that follow the chassis, bound to four chassis points.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
#[br(import(version: u32))]
pub struct SbvSubshape {
    #[br(if(version > 2, 0))]
    pub flag: i32,
    pub count: u32,
    #[br(args { count: count as usize, inner: (version,) })]
    pub nodes: Vec<SbvSubshapeNode>,
    #[br(if(version > 1, [0; 4]))]
    pub quad: [i32; 4],
}

/// A group of points.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct SbvPart {
    pub count: u32,
    #[br(count = count)]
    pub points: Vec<Vector>,
}

/// A wheel mount.
#[derive(BinRead, Debug, Clone, Copy)]
#[brw(little)]
pub struct SbvWheel {
    pub kind: i32,
    pub pos: Vector,
    pub aux: i32,
}

/// A vehicle body file (data/<name>.sbv, read by load_sbv): chassis points and springs, faces, render vertices,
/// subshapes, parts and wheel mounts; version 5 files store the offset everything is centred on.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct VehicleBodyFile {
    pub version: u32,
    #[br(if(version > 4))]
    pub offset: Option<Vector>,

    pub node_count: u32,
    #[br(count = node_count)]
    pub nodes: Vec<SbvNode>,

    pub edge_count: u32,
    #[br(count = edge_count)]
    pub edges: Vec<SbvEdge>,

    pub face_count: u32,
    #[br(count = face_count)]
    pub faces: Vec<SbvFace>,

    pub vertex_count: u32,
    #[br(count = vertex_count)]
    pub vertices: Vec<Vector>,

    pub subshape_count: u32,
    #[br(args { count: subshape_count as usize, inner: (version,) })]
    pub subshapes: Vec<SbvSubshape>,

    pub part_count: u32,
    #[br(count = part_count)]
    pub parts: Vec<SbvPart>,

    pub wheel_count: u32,
    #[br(count = wheel_count)]
    pub wheels: Vec<SbvWheel>,
}

impl VehicleBodyFile {
    pub fn load(path: &Path) -> Result<Self, LoaderError> {
        let bytes = std::fs::read(path)?;
        Ok(VehicleBodyFile::read(&mut Cursor::new(bytes))?)
    }
}
