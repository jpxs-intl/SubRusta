use glam::Vec3;
use rosa_physics::RotMatrix;

use crate::world::{capsule::capsule_intersect_triangle, collide::{calculate_face_normal, segment_intersect_face}, mesh::CUBE_FACES};

const BOX_EDGES: [[usize; 2]; 12] = [[0, 1], [1, 2], [2, 3], [3, 0], [0, 4], [1, 5], [2, 6], [3, 7], [4, 5], [5, 6], [6, 7], [7, 4]];
const PRISM_SIDES: usize = 6;
const PRISM_RINGS: usize = 4;
const PRISM_CAPS: [[usize; 4]; 4] = [[5, 0, 1, 4], [4, 1, 2, 3], [18, 23, 22, 19], [19, 22, 21, 20]];

#[derive(Clone, Debug)]
pub struct HullFace {
    pub verts: [usize; 4],
    pub normal: Vec3,
}

/// A convex collision hull in item-local space (item_types[t] + 0xf08, one face group).
#[derive(Clone, Debug)]
pub struct ConvexHull {
    pub verts: Vec<Vec3>,
    pub faces: Vec<HullFace>,
    pub edges: Vec<[usize; 2]>,
}

impl ConvexHull {
    fn new(verts: Vec<Vec3>, faces: Vec<[usize; 4]>, edges: Vec<[usize; 2]>) -> Self {
        let faces = faces
            .into_iter()
            .map(|f| HullFace { verts: f, normal: calculate_face_normal(verts[f[0]], verts[f[1]], verts[f[2]]) })
            .collect();
        Self { verts, faces, edges }
    }

    /// The default hull of a collidable item: its bounding box.
    pub fn bounding_box(b: Vec3) -> Self {
        let verts = (0..8)
            .map(|k| {
                let q = k & 3;
                Vec3::new(if q == 0 || q == 3 { -b.x } else { b.x }, if k > 3 { b.y } else { -b.y }, if q <= 1 { -b.z } else { b.z })
            })
            .collect();
        Self::new(verts, CUBE_FACES.to_vec(), BOX_EDGES.to_vec())
    }

    /// item_type_build_prism_collision_mesh: four rings of six points (`melon` selects the watermelon's ring radii and heights).
    pub fn prism(wide: f32, narrow: f32, height: f32, melon: bool) -> Self {
        let mut verts = Vec::with_capacity(PRISM_RINGS * PRISM_SIDES);
        let neg_height = -height;
        for ring in 0..PRISM_RINGS {
            let radius = match (melon, ring) {
                (true, 0 | 3) => narrow,
                (true, _) => wide,
                (false, 0 | 1) => wide,
                (false, _) => narrow,
            } as f64;
            let y = match ring {
                0 => neg_height * 0.5,
                1 if melon => neg_height * 0.375,
                1 => height * 0.3125,
                2 => height * 0.375,
                _ => 0.5 * height,
            };
            let (mut sin, mut cos, mut angle) = (0.0f64, 1.0f64, 0.0f32);
            for point in 0..PRISM_SIDES {
                if point > 0 {
                    (sin, cos) = (angle as f64).sin_cos();
                }
                verts.push(Vec3::new((cos * radius) as f32, y, (-sin * radius) as f32));
                angle = (angle as f64 + std::f64::consts::FRAC_PI_3) as f32;
            }
        }

        let mut faces = Vec::with_capacity(22);
        for ring in (0..PRISM_SIDES * 3).step_by(PRISM_SIDES) {
            for k in 0..PRISM_SIDES {
                let next = (k + 1) % PRISM_SIDES;
                faces.push([ring + PRISM_SIDES + k, ring + PRISM_SIDES + next, ring + next, ring + k]);
            }
        }
        faces.extend(PRISM_CAPS);

        let mut edges = Vec::with_capacity(42);
        for ring in (0..PRISM_SIDES * 3).step_by(PRISM_SIDES) {
            for k in 0..PRISM_SIDES {
                edges.push([ring + k, ring + PRISM_SIDES + k]);
            }
        }
        for ring in (0..PRISM_SIDES * PRISM_RINGS).step_by(PRISM_SIDES) {
            for k in 0..PRISM_SIDES {
                edges.push([ring + k, ring + (k + 1) % PRISM_SIDES]);
            }
        }
        Self::new(verts, faces, edges)
    }

    /// transform_collision_mesh_vertices: the hull's vertices placed at `pos` with orientation `rot`.
    pub fn world_verts(&self, pos: Vec3, rot: &RotMatrix) -> Vec<Vec3> {
        let [r0, r1, r2] = *rot;
        self.verts
            .iter()
            .map(|v| {
                Vec3::new(
                    ((v.x * r0.x + v.y * r1.x) + v.z * r2.x) + pos.x,
                    ((r0.y * v.x + r1.y * v.y) + r2.y * v.z) + pos.y,
                    pos.z + ((r0.z * v.x + r1.z * v.y) + r2.z * v.z),
                )
            })
            .collect()
    }

    /// capsule_intersect_hull_face_groups_max_distance: the face hit with the greatest axis distance (zero-distance hits never count).
    pub fn intersect_capsule(&self, world: &[Vec3], start: Vec3, end: Vec3, radius: f32) -> Option<(Vec3, Vec3, f32)> {
        let mut best = (Vec3::ZERO, Vec3::ZERO, 0.0f32);
        for f in &self.faces {
            let [a, b, c, d] = f.verts.map(|i| world[i]);
            for (p, q) in [(b, c), (c, d)] {
                if let Some(h) = capsule_intersect_triangle(start, end, a, p, q, radius)
                    && h.2 > best.2
                {
                    best = h;
                }
            }
        }
        (best.2 > 0.0).then_some(best)
    }
}

/// segment_intersect_plane_two_sided: where the segment crosses the plane through `p` with normal `n`, from either side.
fn segment_intersect_plane_two_sided(n: Vec3, p: Vec3, start: Vec3, end: Vec3) -> Option<Vec3> {
    let d_end = ((p.x - end.x) * n.x + (p.y - end.y) * n.y) + (p.z - end.z) * n.z;
    let d_start = ((p.x - start.x) * n.x + (p.y - start.y) * n.y) + (p.z - start.z) * n.z;
    if (d_start > 0.0 && d_end > 0.0) || (0.0 > d_start && 0.0 > d_end) {
        return None;
    }
    let e = Vec3::new(end.x - start.x, end.y - start.y, end.z - start.z);
    let denom = (n.x * e.x + n.y * e.y) + n.z * e.z;
    if denom == 0.0 {
        return None;
    }
    let t = d_start / denom;
    Some(Vec3::new(e.x * t + start.x, e.y * t + start.y, t * e.z + start.z))
}

/// segment_intersects_triangle_two_sided: whether the segment passes through the triangle `a`, `b`, `c` from either side.
fn segment_intersects_triangle_two_sided(start: Vec3, end: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
    let n = calculate_face_normal(a, b, c);
    let Some(p) = segment_intersect_plane_two_sided(n, a, start, end) else { return false };
    let side = |v: Vec3, w: Vec3| {
        let (pv, wv) = (Vec3::new(p.x - v.x, p.y - v.y, p.z - v.z), Vec3::new(w.x - v.x, w.y - v.y, w.z - v.z));
        Vec3::new(pv.y * wv.z - pv.z * wv.y, pv.z * wv.x - wv.z * pv.x, pv.x * wv.y - pv.y * wv.x)
    };
    let ab = side(a, b);
    if 0.0 > ((ab.x * n.x) + ab.y * n.y) + ab.z * n.z {
        return false;
    }
    let bc = side(b, c);
    if 0.0 > ((bc.x * n.x) + bc.y * n.y) + bc.z * n.z {
        return false;
    }
    let ca = side(c, a);
    ((ca.y * n.y) + ca.x * n.x) + ca.z * n.z >= 0.0
}

/// segment_closest_points_interior: the closest points of two segments' lines, when both lie within their segments.
fn segment_closest_points_interior(a0: Vec3, a1: Vec3, b0: Vec3, b1: Vec3) -> Option<(Vec3, Vec3, f32)> {
    let db = Vec3::new(b1.x - b0.x, b1.y - b0.y, b1.z - b0.z);
    let da = Vec3::new(a1.x - a0.x, a1.y - a0.y, a1.z - a0.z);
    let bb = (db.x * db.x + db.y * db.y) + db.z * db.z;
    let ab = (db.x * da.x + db.y * da.y) + db.z * da.z;
    let aa = (da.x * da.x + da.y * da.y) + da.z * da.z;
    let det = aa * bb - ab * ab;
    if det == 0.0 {
        return None;
    }
    let w = Vec3::new(a0.x - b0.x, a0.y - b0.y, a0.z - b0.z);
    let wb = (w.x * db.x + w.y * db.y) + w.z * db.z;
    let wa = (w.x * da.x + w.y * da.y) + w.z * da.z;
    let s = (ab * wb - wa * bb) / det;
    if 0.0 > s || s > 1.0 {
        return None;
    }
    let t = (ab * s + wb) / bb;
    if 0.0 > t || t > 1.0 {
        return None;
    }
    let ca = Vec3::new(a0.x + da.x * s, da.y * s + a0.y, s * da.z + a0.z);
    let cb = Vec3::new(db.x * t + b0.x, db.y * t + b0.y, b0.z + t * db.z);
    let d = Vec3::new(cb.x - ca.x, cb.y - ca.y, cb.z - ca.z);
    Some((ca, cb, ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt()))
}

/// A contact between two hulls: offsets from each body's position, the normal and the depth. `swapped` contacts come
/// from the second hull's vertices and push the second body along the normal.
pub struct HullContact {
    pub swapped: bool,
    pub offset_a: Vec3,
    pub offset_b: Vec3,
    pub normal: Vec3,
    pub depth: f32,
}

impl ConvexHull {
    /// The centre of the hull's face group (the origin for every item hull) placed at `pos` with orientation `rot`.
    fn world_center(pos: Vec3, rot: &RotMatrix) -> Vec3 {
        let [r0, r1, r2] = *rot;
        let c = Vec3::ZERO;
        Vec3::new(((r0.x * c.x + r1.x * c.y) + r2.x * c.z) + pos.x, ((r0.y * c.x + r1.y * c.y) + r2.y * c.z) + pos.y, ((c.x * r0.z + c.y * r1.z) + c.z * r2.z) + pos.z)
    }

    /// segment_cast_hull_face_groups: the first face the segment enters, as the hit point and the face's world normal.
    pub fn segment_cast(&self, world: &[Vec3], rot: &RotMatrix, start: Vec3, end: Vec3) -> Option<(Vec3, Vec3)> {
        let [r0, r1, r2] = *rot;
        let mut best = (1.0f32, Vec3::ZERO, Vec3::ZERO);
        for f in &self.faces {
            let m = f.normal;
            let n = Vec3::new((r1.x * m.y + r0.x * m.x) + r2.x * m.z, (r1.y * m.y + r0.y * m.x) + r2.y * m.z, (m.x * r0.z + m.y * r1.z) + m.z * r2.z);
            let [a, b, c, d] = f.verts.map(|i| world[i]);
            for (p, q) in [(b, c), (c, d)] {
                if let Some((t, hit)) = segment_intersect_face(n, start, end, a, p, q)
                    && !(best.0 <= t)
                {
                    best = (t, hit, n);
                }
            }
        }
        (1.0 > best.0).then_some((best.1, best.2))
    }
}

/// collide_convex_hulls: the contacts between two item hulls. Each hull's vertices are cast from its centre into the
/// other hull, then crossing edge pairs add a contact at their midpoint.
pub fn collide_convex_hulls(a: &ConvexHull, pos_a: Vec3, rot_a: &RotMatrix, b: &ConvexHull, pos_b: Vec3, rot_b: &RotMatrix) -> Vec<HullContact> {
    let (wa, wb) = (a.world_verts(pos_a, rot_a), b.world_verts(pos_b, rot_b));
    let (ca, cb) = (ConvexHull::world_center(pos_a, rot_a), ConvexHull::world_center(pos_b, rot_b));
    let mut out = Vec::new();
    let sub = |p: Vec3, q: Vec3| Vec3::new(p.x - q.x, p.y - q.y, p.z - q.z);
    let passes = [(false, &wa, ca, b, &wb, rot_b, pos_a, pos_b), (true, &wb, cb, a, &wa, rot_a, pos_b, pos_a)];
    for (swapped, from_w, from_c, into, into_w, into_rot, from_pos, into_pos) in passes {
        for v in from_w.iter() {
            let Some((hp, n)) = into.segment_cast(into_w, into_rot, from_c, *v) else { continue };
            let d = sub(hp, *v);
            let depth = (d.z * n.z) + (n.x * d.x + d.y * n.y);
            out.push(HullContact { swapped, offset_a: sub(hp, from_pos), offset_b: sub(hp, into_pos), normal: n, depth });
        }
    }
    let axis = sub(cb, ca);
    let cull_a: Vec<bool> = a
        .edges
        .iter()
        .map(|e| {
            e.iter().all(|&i| {
                let d = sub(wa[i], ca);
                0.0 > (axis.y * d.y + axis.x * d.x) + axis.z * d.z
            })
        })
        .collect();
    let cull_b: Vec<bool> = b
        .edges
        .iter()
        .map(|e| {
            e.iter().all(|&i| {
                let d = sub(wb[i], cb);
                ((axis.x * d.x + axis.y * d.y) + axis.z * d.z) > 0.0
            })
        })
        .collect();
    for (i, ea) in a.edges.iter().enumerate() {
        let (a0, a1) = (wa[ea[0]], wa[ea[1]]);
        for (j, eb) in b.edges.iter().enumerate() {
            if cull_a[i] && cull_b[j] {
                continue;
            }
            let (b0, b1) = (wb[eb[0]], wb[eb[1]]);
            let Some((pa, pb, dist)) = segment_closest_points_interior(a0, a1, b0, b1) else { continue };
            let d = sub(pb, pa);
            if (d.z * axis.z) + (d.y * axis.y + d.x * axis.x) > 0.0 {
                continue;
            }
            if !segment_intersects_triangle_two_sided(b0, b1, ca, a0, a1) || !segment_intersects_triangle_two_sided(a0, a1, cb, b0, b1) {
                continue;
            }
            let len = (d.z * d.z + (d.y * d.y + d.x * d.x)).sqrt();
            let n = if len == 0.0 {
                Vec3::ZERO
            } else {
                let inv = 1.0 / len;
                Vec3::new(d.x * inv, d.y * inv, inv * d.z)
            };
            let mid = Vec3::new(0.5 * (pb.x + pa.x), 0.5 * (pb.y + pa.y), 0.5 * (pb.z + pa.z));
            out.push(HullContact { swapped: false, offset_a: sub(mid, pos_a), offset_b: sub(mid, pos_b), normal: n, depth: dist });
        }
    }
    out
}
