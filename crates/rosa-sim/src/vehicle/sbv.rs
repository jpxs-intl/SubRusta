use glam::Vec3;
use rosa_map::file_types::sbv::VehicleBodyFile;
use rosa_math::vector::Vector;

use crate::sim::hull::ConvexHull;

const MAX_MESH_EDGES: usize = 256;

/// A vehicle body as load_sbv leaves it: the body file (a version 5 file moved onto its stored offset), the chassis
/// points around their mean (+0x4018) and the collision mesh over them with the quad faces' unique edges.
#[derive(Clone, Debug)]
pub struct VehicleBody {
    pub file: VehicleBodyFile,
    pub centred: Vec<Vec3>,
    pub mesh: ConvexHull,
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

/// register_unique_pair: the mesh's edge list keeps each pair once, in either direction, up to 256 pairs.
fn register_unique_pair(edges: &mut Vec<[usize; 2]>, a: usize, b: usize) {
    if edges.len() >= MAX_MESH_EDGES || edges.iter().any(|&[x, y]| (x == a && y == b) || (x == b && y == a)) {
        return;
    }
    edges.push([a, b]);
}

impl VehicleBody {
    /// load_sbv: reads `path`; None if it cannot be read.
    pub fn load(path: &std::path::Path) -> Option<Self> {
        let mut file = VehicleBodyFile::load(path).ok()?;
        // TODO: version 1 files centre the vertices and parts on the chassis mean and bind each subshape
        // (vehicletype_bind_subshape_nodes); no shipped file is that old
        let positions: Vec<Vec3> = file.nodes.iter().map(|n| n.pos.0).collect();
        let centred = match file.offset {
            Some(Vector(offset)) => {
                let shift = |p: &mut Vector| p.0 = sub(p.0, offset);
                file.nodes.iter_mut().for_each(|n| shift(&mut n.pos));
                file.vertices.iter_mut().for_each(shift);
                file.parts.iter_mut().flat_map(|p| p.points.iter_mut()).for_each(shift);
                file.wheels.iter_mut().for_each(|w| shift(&mut w.pos));
                positions.into_iter().map(|p| sub(p, offset)).collect()
            }
            None => {
                let sum = positions.iter().fold(Vec3::ZERO, |s, p| Vec3::new(s.x + p.x, s.y + p.y, s.z + p.z));
                let inv = 1.0 / positions.len() as f32;
                let m = Vec3::new(sum.x * inv, sum.y * inv, sum.z * inv);
                positions.into_iter().map(|p| sub(p, m)).collect::<Vec<_>>()
            }
        };

        let mut edges = Vec::new();
        for f in file.faces.iter().filter(|f| f.indices.len() == 4) {
            let f: Vec<usize> = f.indices.iter().map(|&i| i as usize).collect();
            for k in 0..4 {
                register_unique_pair(&mut edges, f[k], f[(k + 1) % 4]);
            }
        }
        let faces = file.faces.iter().map(|f| std::array::from_fn(|k| f.indices.get(k).map_or(0, |&i| i as usize))).collect();
        let mesh = ConvexHull::new(centred.clone(), faces, edges);
        Some(Self { file, centred, mesh })
    }
}
