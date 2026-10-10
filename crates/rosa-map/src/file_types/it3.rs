use std::{io::Cursor, path::Path};

use binrw::BinRead;
use rosa_math::vector::Vector;

use super::LoaderError;

/// The version load_it3_file takes, and its two chunks: the render mesh and the collision hull.
const VERSION: i32 = 3;
const MESH_CHUNK: i32 = 0;
const COLLISION_CHUNK: i32 = 1;

/// A corner of a render face: its vertex and texture coordinate.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct It3Corner {
    pub index: i32,
    pub uv: [f32; 2],
}

#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct It3Face {
    pub count: u32,
    #[br(count = count)]
    pub corners: Vec<It3Corner>,
}

/// The render mesh chunk (sub-version 2).
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct It3Mesh {
    pub sub_version: i32,
    pub vert_count: u32,
    #[br(count = vert_count)]
    pub verts: Vec<Vector>,
    pub face_count: u32,
    #[br(count = face_count)]
    pub faces: Vec<It3Face>,
}

#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct It3Indices {
    pub count: u32,
    #[br(count = count)]
    pub items: Vec<i32>,
}

/// A convex group of the hull: its centre, the vertices cast from it and its faces (vertex indices).
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct It3Group {
    pub center: Vector,
    pub verts: It3Indices,
    pub face_count: u32,
    #[br(count = face_count)]
    pub faces: Vec<It3Indices>,
}

/// The collision chunk (sub-version 1): the hull's vertices and its convex groups.
#[derive(BinRead, Debug, Clone)]
#[brw(little)]
pub struct It3Collision {
    pub sub_version: i32,
    pub vert_count: u32,
    #[br(count = vert_count)]
    pub verts: Vec<Vector>,
    pub group_count: u32,
    #[br(count = group_count)]
    pub groups: Vec<It3Group>,
}

/// An item mesh file (data/item/*.it3, load_it3_file): version 3, then a render mesh chunk and a collision chunk,
/// each led by its chunk id.
#[derive(Debug, Clone)]
pub struct It3File {
    pub mesh: Option<It3Mesh>,
    pub collision: Option<It3Collision>,
}

impl It3File {
    pub fn load(path: &Path) -> Result<Self, LoaderError> {
        let bytes = std::fs::read(path)?;
        let mut r = Cursor::new(bytes);
        let version = i32::read_le(&mut r)?;
        if version != VERSION {
            return Ok(It3File { mesh: None, collision: None });
        }
        let mut chunk = i32::read_le(&mut r)?;
        let mesh = if chunk == MESH_CHUNK {
            let m = It3Mesh::read(&mut r)?;
            chunk = i32::read_le(&mut r)?;
            Some(m)
        } else {
            None
        };
        let collision = if chunk == COLLISION_CHUNK { Some(It3Collision::read(&mut r)?) } else { None };
        Ok(It3File { mesh, collision })
    }
}
