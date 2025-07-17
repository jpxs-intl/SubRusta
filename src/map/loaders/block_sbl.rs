use binrw::BinRead;
use rapier3d::{math::Point, na::{Const, OPoint, Point3}, parry::math};

use crate::world::vector::{IntVector, Vector};

pub type RapierBlock = ((Vec<OPoint<f32, Const<3>>>, Vec<[u32; 3]>), Vec<OPoint<f32, Const<3>>>);

#[derive(BinRead, Debug, Clone)]
pub struct BlockSurfaceVertex {
    pub pos: Vector,
    #[br(count = 2)]
    pub tex_coord: Vec<f32>
}

#[derive(BinRead, Debug, Clone)]
pub struct BlockSurface {
    pub order_x: u32,
    pub tess_x: u32,
    pub order_y: u32,
    pub tess_y: u32,
    pub texture: u32,
    pub project_texture: u32,

    #[br(count = 16)]
    pub vertex_data: Vec<BlockSurfaceVertex>
}

#[derive(BinRead, Debug, Clone)]
pub struct BlockBox {
    #[br(count = 8)]
    pub vertices: Vec<Vector>,
    #[br(count = 6)]
    pub texture: Vec<u32>,
    pub side_flag: u32
}

#[derive(BinRead, Debug, Clone)]
#[br(little)]
/// This is **ONLY** compatible with the CSX Version
/// Reason being the file version restricts boxes to version 2
/// but the CSX makes version 1 have boxes
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
    pub surfaces: Vec<BlockSurface>,

    pub box_count: u32,
    #[br(count = box_count)]
    pub boxes: Vec<BlockBox>
}

impl BlockFile {
    pub fn cubes_to_rapier(&self) -> RapierBlock {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for boxes in &self.boxes {
            BlockFile::insert_cube(boxes, &mut vertices, &mut indices);
        }

        let rapier_vertices: Vec<OPoint<f32, Const<3>>> = vertices
            .iter()
            .map(|v| Point::from(math::Vector::new(v.x, v.y, v.z)))
            .collect();
        
        let rapier_indices: Vec<[u32; 3]> = indices
            .chunks(3)
            .map(|chunk| [chunk[0], chunk[1], chunk[2]])
            .collect();

        let triangles = (rapier_vertices, rapier_indices);

        let mut points = vec![];
        for surface in &self.surfaces {
            for vertex in &surface.vertex_data {
                let point = Point3::new(vertex.pos.x * 4.0, vertex.pos.y * 4.0, vertex.pos.z * 4.0);

                points.push(point);
            }
        }

        (triangles, points)
    }

    fn insert_cube(box_data: &BlockBox, vertices: &mut Vec<Vector>, indices: &mut Vec<u32>) {
        let base = vertices.len() as u32;

        for vertex in &box_data.vertices {
            vertices.push(*vertex * 4.0)
        }

        let cube_faces = [
            ([0, 1, 2], [0, 2, 3]),
            ([4, 6, 5], [4, 7, 6]),
            ([0, 4, 5], [0, 5, 1]),
            ([2, 6, 7], [2, 7, 3]),
            ([0, 3, 7], [0, 7, 4]),
            ([1, 5, 6], [1, 6, 2])
        ];

        for (tri1, tri2) in cube_faces.iter() {
            indices.push(base + tri1[0]);
            indices.push(base + tri1[1]);
            indices.push(base + tri1[2]);

            indices.push(base + tri2[0]);
            indices.push(base + tri2[1]);
            indices.push(base + tri2[2]);
        }
    }
}