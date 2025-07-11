use binrw::BinRead;

use crate::world::vector::{IntVector, Vector};

#[derive(BinRead, Debug, Clone)]
pub struct BlockSurfaceVertex {
    pub pos: Vector,
    #[br(count = 2)]
    pub tex_coord: Vec<f32>
}

#[derive(BinRead, Debug, Clone)]
pub struct BlockSurface {
    pub order_x: u32,
    pub order_y: u32,
    pub tess_x: u32,
    pub tess_y: u32,
    pub texture: u32,
    pub project_texture: u32,

    pub vertex_data: [[BlockSurfaceVertex; 4]; 4]
}

#[derive(BinRead, Debug, Clone)]
pub struct BlockBox {
    #[br(count = 8)]
    pub pos: Vec<Vector>,
    #[br(count = 6)]
    pub texture: Vec<u32>,
    pub side_flag: u32
}

#[derive(BinRead, Debug, Clone)]
#[br(little)]
pub struct BlockFile {
    pub version: u32,
    pub size: IntVector,
    pub floor: u32,
    pub ceiling: u32,
    pub wall_pz: u32,
    pub wall_nx: u32,
    pub wall_px: u32,
    pub wall_nz: u32,

    pub surface_count: u32,
    #[br(count = surface_count)]
    pub block_surface: Vec<BlockSurface>,

    pub box_count: u32,
    #[br(count = box_count)]
    pub boxes: Vec<BlockBox>
}