use std::sync::LazyLock;

use glam::{IVec3, Vec3};
use rosa_physics::{
    RotMatrix,
    rotation::{IDENTITY, rotate_orientation, rotate_vector_about_axis},
};

use crate::world::{
    capsule::capsule_intersect_triangle,
    collide::{calculate_face_normal, segment_intersect_face},
    mesh::{CUBE_CORNERS, CUBE_FACES},
    sphere_cast::sphere_project_onto_triangle,
};

/// The garage door of a corporation base.
pub const GARAGE_DOOR: u32 = 6;
const TYPES: usize = 8;
/// An object cell faces one of four ways, a quarter turn apart.
const TURNS: [f32; 3] = [1.570_796_4, 3.141_592_7, 4.712_389];

/// An area object type (setup_city_object_types, 0x38f9d8e0 + type * 0x2418): the radius it is culled by, its boxes and
/// where it sits from its cell's floor centre.
#[derive(Default)]
pub struct CityObjectType {
    pub radius: f32,
    pub verts: Vec<Vec3>,
    pub quads: Vec<[usize; 4]>,
    pub offset: Vec3,
}

impl CityObjectType {
    /// cityobject_add_box: a box of `size` around `centre`.
    fn add_box(&mut self, centre: Vec3, size: Vec3) {
        let base = self.verts.len();
        self.verts.extend(CUBE_CORNERS.iter().map(|c| {
            Vec3::new((c.x - 0.5) * size.x + centre.x, (c.y - 0.5) * size.y + centre.y, (c.z - 0.5) * size.z + centre.z)
        }));
        self.quads.extend(CUBE_FACES.iter().map(|f| f.map(|i| i + base)));
    }
}

pub static CITY_OBJECT_TYPES: LazyLock<Vec<CityObjectType>> = LazyLock::new(|| {
    let mut t: Vec<CityObjectType> = (0..TYPES).map(|_| CityObjectType::default()).collect();
    t[6].add_box(Vec3::new(0.0, 2.0, -1.625), Vec3::new(4.0, 4.0, 0.25));
    t[6].radius = 3.5;
    t[1].add_box(Vec3::ZERO, Vec3::new(0.375, 6.0, 0.375));
    t[1].add_box(Vec3::new(0.0, 3.0, -5.0), Vec3::new(0.25, 0.25, 10.0));
    t[1].add_box(Vec3::new(0.0, 3.0, -6.0), Vec3::new(0.3125, 1.0, 0.5));
    t[1].add_box(Vec3::new(0.0, 3.0, -10.0), Vec3::new(0.3125, 1.0, 0.5));
    t[1].offset = Vec3::new(0.0, 3.25, -1.0);
    t[1].radius = 3.0;
    t[4].add_box(Vec3::new(0.0, 2.625, -8.0), Vec3::new(0.0625, 0.5, 2.0));
    t
});

/// Where the object in an area object cell word sits (in the area's frame) and how it is turned: its type's offset turned
/// with it, from the centre of the cell's floor.
pub fn object_pose(word: u32, cell: IVec3, size: f32) -> (usize, Vec3, RotMatrix) {
    let kind = (word & 127) as usize;
    let turn = ((word as i32 >> 10) & 3) as usize;
    let mut offset = CITY_OBJECT_TYPES[kind].offset;
    if turn != 0 {
        offset = rotate_vector_about_axis(offset, Vec3::Y, TURNS[turn - 1]);
    }
    let half = 0.5 * size;
    let pos = Vec3::new((cell.x as f32 * size + half) + offset.x, cell.y as f32 * size + offset.y, (cell.z as f32 * size + half) + offset.z);
    let mut rot = IDENTITY;
    if turn != 0 {
        rotate_orientation(&mut rot, Vec3::Y, TURNS[turn - 1]);
    }
    (kind, pos, rot)
}

fn turn(m: &RotMatrix, v: Vec3) -> Vec3 {
    let [a, b, c] = *m;
    Vec3::new((v.y * b.x + v.x * a.x) + v.z * c.x, (v.y * b.y + v.x * a.y) + v.z * c.y, (v.y * b.z + v.x * a.z) + v.z * c.z)
}

/// Whether the swept range `start..end` (grown by `r`) stays clear of the object's culling box on some axis.
fn culled(t: &CityObjectType, pos: Vec3, start: Vec3, end: Vec3, r: f32) -> bool {
    let (p, s, e) = (pos.to_array(), start.to_array(), end.to_array());
    (0..3).any(|k| p[k] - t.radius > s[k] + r && p[k] - t.radius > e[k] + r) || (0..3).any(|k| s[k] - r > p[k] + t.radius && e[k] - r > p[k] + t.radius)
}

/// segment_intersect_area_object_mesh: the segment against the object's faces, each face's normal taken before the
/// object is moved into place. Returns the fraction, point and normal of the nearest hit.
pub fn segment_intersect_object(kind: usize, pos: Vec3, rot: &RotMatrix, start: Vec3, end: Vec3) -> Option<(f32, Vec3, Vec3)> {
    let t = &CITY_OBJECT_TYPES[kind];
    if culled(t, pos, start, end, 0.0) {
        return None;
    }
    let mut best = 1.0f32;
    let mut out = (Vec3::ZERO, Vec3::ZERO);
    for q in &t.quads {
        let [a, b, c, d] = q.map(|i| turn(rot, t.verts[i]));
        let n = calculate_face_normal(a, b, c);
        let [a, b, c, d] = [a, b, c, d].map(|v| Vec3::new(v.x + pos.x, v.y + pos.y, v.z + pos.z));
        for (p, q) in [(b, c), (c, d)] {
            if let Some((f, hit)) = segment_intersect_face(n, start, end, a, p, q)
                && !(best <= f)
            {
                best = f;
                out = (hit, n);
            }
        }
    }
    (1.0 > best).then_some((best, out.0, out.1))
}

/// capsule_intersect_level_shape_mesh: the capsule against the object's faces. Returns the point, normal and axis
/// distance of the nearest contact.
pub fn capsule_intersect_object(kind: usize, pos: Vec3, rot: &RotMatrix, start: Vec3, end: Vec3, radius: f32) -> Option<(Vec3, Vec3, f32)> {
    let t = &CITY_OBJECT_TYPES[kind];
    if culled(t, pos, start, end, radius) {
        return None;
    }
    let mut best = 65536.0f32;
    let mut out = (Vec3::ZERO, Vec3::ZERO);
    for q in &t.quads {
        let [a, b, c, d] = q.map(|i| {
            let v = turn(rot, t.verts[i]);
            Vec3::new(v.x + pos.x, v.y + pos.y, v.z + pos.z)
        });
        for (p, q) in [(b, c), (c, d)] {
            if let Some((hit, n, dist)) = capsule_intersect_triangle(start, end, a, p, q, radius)
                && best > dist
            {
                best = dist;
                out = (hit, n);
            }
        }
    }
    (65536.0 > best).then_some((out.0, out.1, best))
}

/// sphere_intersect_level_shape_mesh: a sphere at rest against the object's faces (no edges), each face's normal taken
/// before the object is moved into place. Returns the point, normal and distance of the nearest contact.
pub fn sphere_intersect_object(kind: usize, pos: Vec3, rot: &RotMatrix, center: Vec3, radius: f32) -> Option<(Vec3, Vec3, f32)> {
    let t = &CITY_OBJECT_TYPES[kind];
    if culled(t, pos, center, center, radius) {
        return None;
    }
    let mut best = 65536.0f32;
    let mut out = (Vec3::ZERO, Vec3::ZERO);
    for q in &t.quads {
        let [a, b, c, d] = q.map(|i| turn(rot, t.verts[i]));
        let n = calculate_face_normal(a, b, c);
        let [a, b, c, d] = [a, b, c, d].map(|v| Vec3::new(v.x + pos.x, v.y + pos.y, v.z + pos.z));
        for (p, q) in [(b, c), (c, d)] {
            if let Some((hit, dist)) = sphere_project_onto_triangle(center, n, a, p, q, radius)
                && !(best <= dist)
            {
                best = dist;
                out = (hit, n);
            }
        }
    }
    (65536.0 > best).then_some((out.0, out.1, best))
}
