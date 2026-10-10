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

/// A convex group of a hull (0x38 each from the hull's +0x18): its centre, the vertices cast from it into another
/// hull, its faces and its edges.
#[derive(Clone, Debug)]
pub struct HullGroup {
    pub center: Vec3,
    pub verts: Vec<usize>,
    pub faces: Vec<HullFace>,
    pub edges: Vec<[usize; 2]>,
}

/// A convex collision hull in item-local space (item_types[t] + 0xf08): its vertices and its face groups.
#[derive(Clone, Debug)]
pub struct ConvexHull {
    pub verts: Vec<Vec3>,
    pub groups: Vec<HullGroup>,
}

impl ConvexHull {
    /// A hull of one group around the origin, cast from every vertex.
    pub(crate) fn new(verts: Vec<Vec3>, faces: Vec<[usize; 4]>, edges: Vec<[usize; 2]>) -> Self {
        let faces = faces
            .into_iter()
            .map(|f| HullFace { verts: f, normal: calculate_face_normal(verts[f[0]], verts[f[1]], verts[f[2]]) })
            .collect();
        let group = HullGroup { center: Vec3::ZERO, verts: (0..verts.len()).collect(), faces, edges };
        Self { verts, groups: vec![group] }
    }

    /// load_it3_file's hull: the file's vertices and groups, each face's normal taken from its second to fourth
    /// corners, and every group given the box edges.
    pub fn from_it3(c: &rosa_map::file_types::it3::It3Collision) -> Self {
        let verts: Vec<Vec3> = c.verts.iter().map(|v| v.0).collect();
        let index = |i: i32| usize::try_from(i).unwrap_or(0).min(verts.len().saturating_sub(1));
        let groups = c
            .groups
            .iter()
            .map(|g| {
                let faces = g
                    .faces
                    .iter()
                    .map(|f| {
                        let corners: [usize; 4] = std::array::from_fn(|k| index(f.items.get(k).copied().unwrap_or(0)));
                        let normal = calculate_face_normal(verts[corners[1]], verts[corners[2]], verts[corners[3]]);
                        HullFace { verts: corners, normal }
                    })
                    .collect();
                HullGroup { center: g.center.0, verts: g.verts.items.iter().map(|&i| index(i)).collect(), faces, edges: BOX_EDGES.to_vec() }
            })
            .collect();
        Self { verts, groups }
    }

    /// Every face of every group, in order.
    pub(crate) fn faces(&self) -> impl Iterator<Item = &HullFace> {
        self.groups.iter().flat_map(|g| &g.faces)
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
                angle = (angle as f64 + 60.0_f64.to_radians()) as f32;
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
                v.x * r0 + v.y * r1 + v.z * r2 + pos
            })
            .collect()
    }

    /// capsule_intersect_hull_face_groups_max_distance: the face hit with the greatest axis distance (zero-distance hits never count).
    pub fn intersect_capsule(&self, world: &[Vec3], start: Vec3, end: Vec3, radius: f32) -> Option<(Vec3, Vec3, f32)> {
        let mut best = (Vec3::ZERO, Vec3::ZERO, 0.0f32);
        for f in self.faces() {
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
    let e = end - start;
    let denom = n.dot(e);
    if denom == 0.0 {
        return None;
    }
    let t = d_start / denom;
    Some(e * t + start)
}

/// segment_intersects_triangle_two_sided: whether the segment passes through the triangle `a`, `b`, `c` from either side.
fn segment_intersects_triangle_two_sided(start: Vec3, end: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
    let n = calculate_face_normal(a, b, c);
    let Some(p) = segment_intersect_plane_two_sided(n, a, start, end) else { return false };
    let side = |v: Vec3, w: Vec3| {
        let (pv, wv) = (p - v, w - v);
        pv.cross(wv)
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
    let db = b1 - b0;
    let da = a1 - a0;
    let bb = db.length_squared();
    let ab = db.dot(da);
    let aa = da.length_squared();
    let det = aa * bb - ab * ab;
    if det == 0.0 {
        return None;
    }
    let w = a0 - b0;
    let wb = w.dot(db);
    let wa = w.dot(da);
    let s = (ab * wb - wa * bb) / det;
    if 0.0 > s || s > 1.0 {
        return None;
    }
    let t = (ab * s + wb) / bb;
    if 0.0 > t || t > 1.0 {
        return None;
    }
    let ca = a0 + da * s;
    let cb = db * t + b0;
    let d = cb - ca;
    Some((ca, cb, d.length()))
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
    /// The centre `c` of a face group placed at `pos` with orientation `rot`.
    fn world_center(c: Vec3, pos: Vec3, rot: &RotMatrix) -> Vec3 {
        let [r0, r1, r2] = *rot;
        r0 * c.x + r1 * c.y + r2 * c.z + pos
    }

    /// trace_segment_item_mesh: the segment against each face of the hull placed as `world`, both triangles of a face
    /// under the first one's normal; the nearest hit as (fraction, point, normal).
    pub fn trace_mesh(&self, world: &[Vec3], start: Vec3, end: Vec3) -> Option<(f32, Vec3, Vec3)> {
        let mut best = (1.0f32, Vec3::ZERO, Vec3::ZERO);
        for f in self.faces() {
            let [a, b, c, d] = f.verts.map(|i| world[i]);
            let n = calculate_face_normal(a, b, c);
            for (p, q) in [(b, c), (c, d)] {
                if let Some((t, hit)) = segment_intersect_face(n, start, end, a, p, q)
                    && !(best.0 <= t)
                {
                    best = (t, hit, n);
                }
            }
        }
        (1.0 > best.0).then_some(best)
    }

    /// segment_cast_hull_face_groups: the first face the segment enters, as the hit point and the face's world normal.
    pub fn segment_cast(&self, world: &[Vec3], rot: &RotMatrix, start: Vec3, end: Vec3) -> Option<(Vec3, Vec3)> {
        let [r0, r1, r2] = *rot;
        let mut best = (1.0f32, Vec3::ZERO, Vec3::ZERO);
        for f in self.faces() {
            let m = f.normal;
            let n = r1 * m.y + r0 * m.x + r2 * m.z;
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

/// collide_convex_hulls: the contacts between two item hulls. The vertices of each of a hull's groups are cast from
/// the group's centre into the other hull, then for each pair of groups the crossing edge pairs add a contact at their
/// midpoint.
pub fn collide_convex_hulls(a: &ConvexHull, pos_a: Vec3, rot_a: &RotMatrix, b: &ConvexHull, pos_b: Vec3, rot_b: &RotMatrix) -> Vec<HullContact> {
    let (wa, wb) = (a.world_verts(pos_a, rot_a), b.world_verts(pos_b, rot_b));
    let mut out = Vec::new();
    let passes = [(false, a, &wa, rot_a, b, &wb, rot_b, pos_a, pos_b), (true, b, &wb, rot_b, a, &wa, rot_a, pos_b, pos_a)];
    for (swapped, from, from_w, from_rot, into, into_w, into_rot, from_pos, into_pos) in passes {
        for g in &from.groups {
            let from_c = ConvexHull::world_center(g.center, from_pos, from_rot);
            for &i in &g.verts {
                let v = from_w[i];
                let Some((hp, n)) = into.segment_cast(into_w, into_rot, from_c, v) else { continue };
                let d = hp - v;
                let depth = n.dot(d);
                out.push(HullContact { swapped, offset_a: (hp - from_pos), offset_b: (hp - into_pos), normal: n, depth });
            }
        }
    }
    for ga in &a.groups {
        let ca = ConvexHull::world_center(ga.center, pos_a, rot_a);
        for gb in &b.groups {
            let cb = ConvexHull::world_center(gb.center, pos_b, rot_b);
            collide_group_edges(ga, &wa, ca, gb, &wb, cb, pos_a, pos_b, &mut out);
        }
    }
    out
}

/// The edge part of collide_convex_hulls for one pair of groups: edges facing away from the other group's centre on
/// both sides are skipped; crossing edges whose closest points face the right way add a contact at their midpoint.
#[allow(clippy::too_many_arguments)]
fn collide_group_edges(ga: &HullGroup, wa: &[Vec3], ca: Vec3, gb: &HullGroup, wb: &[Vec3], cb: Vec3, pos_a: Vec3, pos_b: Vec3, out: &mut Vec<HullContact>) {
    let axis = cb - ca;
    let cull_a: Vec<bool> = ga
        .edges
        .iter()
        .map(|e| {
            e.iter().all(|&i| {
                let d = wa[i] - ca;
                0.0 > (axis.y * d.y + axis.x * d.x) + axis.z * d.z
            })
        })
        .collect();
    let cull_b: Vec<bool> = gb
        .edges
        .iter()
        .map(|e| {
            e.iter().all(|&i| {
                let d = wb[i] - cb;
                (axis.dot(d)) > 0.0
            })
        })
        .collect();
    for (i, ea) in ga.edges.iter().enumerate() {
        let (a0, a1) = (wa[ea[0]], wa[ea[1]]);
        for (j, eb) in gb.edges.iter().enumerate() {
            if cull_a[i] && cull_b[j] {
                continue;
            }
            let (b0, b1) = (wb[eb[0]], wb[eb[1]]);
            let Some((pa, pb, dist)) = segment_closest_points_interior(a0, a1, b0, b1) else { continue };
            let d = pb - pa;
            if (d.z * axis.z) + (d.y * axis.y + d.x * axis.x) > 0.0 {
                continue;
            }
            if !segment_intersects_triangle_two_sided(b0, b1, ca, a0, a1) || !segment_intersects_triangle_two_sided(a0, a1, cb, b0, b1) {
                continue;
            }
            let n = d.normalize_or_zero();
            let mid = 0.5 * (pb + pa);
            out.push(HullContact { swapped: false, offset_a: (mid - pos_a), offset_b: (mid - pos_b), normal: n, depth: dist });
        }
    }
}
