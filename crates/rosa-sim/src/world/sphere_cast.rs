use glam::{IVec3, Vec3};

use crate::world::{
    area::{AreaGrid, CUBE, FOOTPRINT, MESH, TYPE_MASK, cell_index},
    city_objects::{object_pose, sphere_intersect_object},
    collide::{calculate_face_normal, segment_intersect_face},
    mesh::{CUBE_CORNERS, CUBE_FACES, CUBE_NORMALS},
    meshes::BlockMeshes,
    trace::{AreaFrame, cvtt},
};

const NO_HIT: f32 = 65536.0;

/// The nearest contact so far: point, normal and distance from the centre.
struct Best {
    pos: Vec3,
    normal: Vec3,
    dist: f32,
}

impl Best {
    fn new() -> Self {
        Self { pos: Vec3::ZERO, normal: Vec3::ZERO, dist: NO_HIT }
    }

    fn take(&mut self, hit: Option<(Vec3, Vec3, f32)>) {
        if let Some((pos, normal, dist)) = hit
            && !(self.dist <= dist)
        {
            *self = Self { pos, normal, dist };
        }
    }

    fn hit(self) -> Option<(Vec3, Vec3, f32)> {
        (NO_HIT > self.dist).then_some((self.pos, self.normal, self.dist))
    }
}

fn dot(d: Vec3, n: Vec3) -> f32 {
    (d.x * n.x + d.y * n.y) + d.z * n.z
}

/// sphere_project_onto_triangle_face: a centre in front of the face within `radius` touches it where it drops onto the
/// face along the normal. Returns the point and its distance.
pub fn sphere_project_onto_triangle(center: Vec3, n: Vec3, a: Vec3, b: Vec3, c: Vec3, radius: f32) -> Option<(Vec3, f32)> {
    let d = dot(Vec3::new(center.x - a.x, center.y - a.y, center.z - a.z), n);
    if d > radius || 0.0 > d {
        return None;
    }
    let r = -radius;
    let end = Vec3::new(n.x * r + center.x, n.y * r + center.y, r * n.z + center.z);
    let (_, p) = segment_intersect_face(n, center, end, a, b, c)?;
    let (x, y, z) = (p.x - center.x, p.y - center.y, p.z - center.z);
    Some((p, (z * z + (x * x + y * y)).sqrt()))
}

/// sphere_intersect_segment_interior: the point of an edge nearest the centre, if it lies within the edge and within
/// `radius`. Returns the point, the normal from it to the centre and the distance.
pub fn sphere_intersect_segment(center: Vec3, p0: Vec3, p1: Vec3, radius: f32) -> Option<(Vec3, Vec3, f32)> {
    let e = Vec3::new(p1.x - p0.x, p1.y - p0.y, p1.z - p0.z);
    let len = ((e.x * e.x + e.y * e.y) + e.z * e.z).sqrt();
    let v = Vec3::new(center.x - p0.x, center.y - p0.y, center.z - p0.z);
    let u = if len != 0.0 {
        let inv = 1.0 / len;
        Vec3::new(e.x * inv, e.y * inv, e.z * inv)
    } else {
        Vec3::ZERO
    };
    let t = (v.x * u.x + v.y * u.y) + v.z * u.z;
    if !(t > 0.0) || !(len > t) {
        return None;
    }
    let q = Vec3::new(u.x * t + p0.x, u.y * t + p0.y, t * u.z + p0.z);
    let d = Vec3::new(center.x - q.x, center.y - q.y, center.z - q.z);
    let dist = ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt();
    if !(radius > dist) {
        return None;
    }
    let n = if dist != 0.0 {
        let inv = 1.0 / dist;
        Vec3::new(d.x * inv, d.y * inv, d.z * inv)
    } else {
        Vec3::ZERO
    };
    Some((q, n, dist))
}

/// A face the centre is in front of: its two triangles, then its four edges.
fn quad(best: &mut Best, center: Vec3, n: Vec3, [a, b, c, d]: [Vec3; 4], radius: f32) {
    if !(dot(Vec3::new(center.x - a.x, center.y - a.y, center.z - a.z), n) > 0.0) {
        return;
    }
    best.take(sphere_project_onto_triangle(center, n, a, b, c, radius).map(|(p, d)| (p, n, d)));
    best.take(sphere_project_onto_triangle(center, n, a, c, d, radius).map(|(p, d)| (p, n, d)));
    for (p, q) in [(a, b), (b, c), (c, d), (d, a)] {
        best.take(sphere_intersect_segment(center, p, q, radius));
    }
}

/// sphere_intersect_voxel_cube_faces: the cube's open faces (those the cell word marks).
fn cube_faces(center: Vec3, cell: IVec3, s: f32, v: u32, radius: f32) -> Option<(Vec3, Vec3, f32)> {
    let base = Vec3::new(cell.x as f32 * s, cell.y as f32 * s, cell.z as f32 * s);
    let corner = |i: usize| {
        let c = CUBE_CORNERS[i];
        Vec3::new(c.x * s + base.x, c.y * s + base.y, c.z * s + base.z)
    };
    let mut best = Best::new();
    for r in 0..6 {
        if v & (1 << r) == 0 || v & (64 << r) == 0 {
            continue;
        }
        quad(&mut best, center, CUBE_NORMALS[r], CUBE_FACES[r].map(corner), radius);
    }
    best.hit()
}

/// sphere_intersect_voxel_custom_shape: a custom block's faces, then its walls from whichever side the centre is on.
fn custom_shape(meshes: &BlockMeshes, center: Vec3, cell: IVec3, s: f32, v: u32, radius: f32) -> Option<(Vec3, Vec3, f32)> {
    let mesh = meshes.get(v & 65535)?;
    let base = Vec3::new(cell.x as f32 * s, cell.y as f32 * s, cell.z as f32 * s);
    let sc = |p: Vec3| Vec3::new(p.x * s + base.x, p.y * s + base.y, p.z * s + base.z);
    let mut best = Best::new();
    for q in &mesh.quads {
        let [a, b, c, d] = q.map(|i| sc(mesh.verts[i as usize]));
        quad(&mut best, center, calculate_face_normal(a, b, c), [a, b, c, d], radius);
    }
    let skip = (v >> 16) & 1023;
    for (i, w) in mesh.walls.iter().enumerate() {
        if skip & (1 << (i & 31)) != 0 {
            continue;
        }
        let [a, b, c, d] = w.map(sc);
        let n = calculate_face_normal(a, b, c);
        if dot(Vec3::new(center.x - a.x, center.y - a.y, center.z - a.z), n) > 0.0 {
            quad(&mut best, center, n, [a, b, c, d], radius);
        } else {
            quad(&mut best, center, calculate_face_normal(d, c, b), [d, c, b, a], radius);
        }
    }
    best.hit()
}

/// collision_cast_area: a sphere at rest against the cells of the level area it overlaps. An area object it touches
/// ends the search at once, its contact left in the area's frame as the binary leaves it.
pub fn collision_cast_area(area: &AreaGrid, meshes: &BlockMeshes, center: Vec3, radius: f32) -> Option<(Vec3, Vec3, f32)> {
    // TODO: item-set objects (+0x1400) are tested before the area objects
    let f = AreaFrame::new(area);
    let (c3, mn, mx) = (center.to_array(), f.min.to_array(), f.max.to_array());
    if (0..3).any(|k| mn[k] > c3[k] + radius) || (0..3).any(|k| c3[k] - radius > mx[k]) {
        return None;
    }
    let lc = f.to_local(center);
    let la = lc.to_array();
    let lo = la.map(|v| cvtt((v - radius) * f.inv));
    let hi = la.map(|v| cvtt((v + radius) * f.inv));
    let mut best = Best::new();
    for y in lo[1]..=hi[1] {
        for z in lo[2]..=hi[2] {
            for x in lo[0]..=hi[0] {
                let Some(rec) = area.record(x, y, z) else { continue };
                let cell = IVec3::new(x, y, z);
                let idx = cell_index(x, y, z);
                let object = rec.object[idx];
                if object as i32 > 0 {
                    let (kind, pos, rot) = object_pose(object, cell, f.size);
                    if let Some(hit) = sphere_intersect_object(kind, pos, &rot, lc, radius) {
                        return Some(hit);
                    }
                }
                for layer in 0..2 {
                    let v = if layer == 0 { rec.layer0[idx] } else { rec.layer1[idx] };
                    let hit = match v & TYPE_MASK {
                        CUBE => cube_faces(lc, cell, f.size, v, radius),
                        MESH => custom_shape(meshes, lc, cell, f.size, v, radius),
                        FOOTPRINT => {
                            let o = cell - IVec3::new((v & 255) as i32, ((v >> 16) & 255) as i32, ((v >> 8) & 255) as i32);
                            let w = if layer == 0 { area.layer0(o.x, o.y, o.z) } else { area.layer1(o.x, o.y, o.z) };
                            custom_shape(meshes, lc, o, f.size, w, radius)
                        }
                        _ => None,
                    };
                    best.take(hit.map(|(p, n, d)| {
                        let p = f.rotate_back(p);
                        (Vec3::new(p.x + f.origin.x, p.y + f.origin.y, p.z + f.origin.z), f.rotate_back(n), d)
                    }));
                }
            }
        }
    }
    best.hit()
}

/// sphere_cast_level: a sphere at rest against the level's areas (not the terrain). Returns the contact point, normal and
/// distance from the centre.
pub fn sphere_cast_level(area: &AreaGrid, meshes: &BlockMeshes, center: Vec3, radius: f32) -> Option<(Vec3, Vec3, f32)> {
    let mut best = Best::new();
    best.take(collision_cast_area(area, meshes, center, radius));
    best.hit()
}
