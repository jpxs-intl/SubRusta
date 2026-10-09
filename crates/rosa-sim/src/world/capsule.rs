use glam::{IVec3, Vec3};

use crate::world::{
    area::{AreaGrid, CUBE, FOOTPRINT, MESH, TYPE_MASK, cell_index},
    collide::{calculate_face_normal, segment_intersect_face},
    ground::{Ground, ORIGIN},
    mesh::{CUBE_CORNERS, CUBE_FACES},
    meshes::BlockMeshes,
    trace::{AreaFrame, cvtt},
};

const NO_HIT: f32 = 65536.0;
const MAX_MESH_VERTS: usize = 2048;
const MAX_MESH_FACES: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapsuleHit {
    pub pos: Vec3,
    pub normal: Vec3,
    pub dist: f32,
    pub area: i32,
    pub block: IVec3,
    pub cell: u32,
    pub face_attr: u32,
}

fn dist(a: Vec3, b: Vec3) -> f32 {
    let (x, y, z) = (a.x - b.x, a.y - b.y, a.z - b.z);
    (z * z + (x * x + y * y)).sqrt()
}

/// Closest points between segments `p0..p1` and `q0..q1`; returns (within radius, point on p, point on q, distance).
pub fn segment_closest_points(p0: Vec3, p1: Vec3, q0: Vec3, q1: Vec3, radius: f32) -> (bool, Vec3, Vec3, f32) {
    let d1 = Vec3::new(p1.x - p0.x, p1.y - p0.y, p1.z - p0.z);
    let r = Vec3::new(p0.x - q0.x, p0.y - q0.y, p0.z - q0.z);
    let d2 = Vec3::new(q1.x - q0.x, q1.y - q0.y, q1.z - q0.z);
    let a = (d1.x * d1.x + d1.y * d1.y) + d1.z * d1.z;
    let e = (d2.x * d2.x + d2.y * d2.y) + d2.z * d2.z;
    let c = (r.x * d1.x + r.y * d1.y) + r.z * d1.z;
    let f = (r.x * d2.x + r.y * d2.y) + r.z * d2.z;
    let b = (d2.x * d1.x + d2.y * d1.y) + d2.z * d1.z;
    let denom = e * a - b * b;

    let (s, bs) = if denom != 0.0 {
        let s = (b * f - c * e) / denom;
        if 0.0 > s {
            (0.0, b * 0.0)
        } else if !(s > 1.0) {
            (s, s * b)
        } else {
            (1.0, b)
        }
    } else {
        (0.0, b * 0.0)
    };

    let t = (f + bs) / e;
    let (pd, qd) = if 0.0 > t {
        let s = -c / a;
        let qd = d2 * 0.0;
        let pd = if 0.0 > s { d1 * 0.0 } else if s > 1.0 { d1 } else { d1 * s };
        (pd, qd)
    } else if !(t > 1.0) {
        (d1 * s, d2 * t)
    } else {
        let s = (b - c) / a;
        let pd = if 0.0 > s { d1 * 0.0 } else if !(s > 1.0) { d1 * s } else { d1 };
        (pd, d2)
    };

    let on_p = Vec3::new(pd.x + p0.x, pd.y + p0.y, pd.z + p0.z);
    let on_q = Vec3::new(qd.x + q0.x, qd.y + q0.y, qd.z + q0.z);
    let (dx, dy, dz) = (on_q.x - on_p.x, on_q.y - on_p.y, on_q.z - on_p.z);
    let d = ((dx * dx + dy * dy) + dz * dz).sqrt();
    (radius > d, on_p, on_q, d)
}

/// Capsule `start..end` with `radius` against triangle `a, b, c`: (contact point, normal, axis distance).
pub fn capsule_intersect_triangle(start: Vec3, end: Vec3, a: Vec3, b: Vec3, c: Vec3, radius: f32) -> Option<(Vec3, Vec3, f32)> {
    let n = calculate_face_normal(a, b, c);
    let d0 = ((start.y - a.y) * n.y + (start.x - a.x) * n.x) + (start.z - a.z) * n.z;
    let d1 = ((end.x - a.x) * n.x + (end.y - a.y) * n.y) + (end.z - a.z) * n.z;
    if d0 >= radius && d1 >= radius {
        return None;
    }
    if 0.0 > d0 && 0.0 > d1 {
        return None;
    }

    if !(d0 > 0.0) || !(d1 > 0.0) {
        if let Some((_, p)) = segment_intersect_face(n, start, end, a, b, c) {
            return Some((p, n, 0.0));
        }
        if let Some((_, p)) = segment_intersect_face(n, end, start, a, b, c) {
            return Some((p, n, 0.0));
        }
    } else {
        let k = -radius * 1.125;
        let push = Vec3::new(n.x * k, n.y * k, n.z * k);
        let from = if d1 > d0 { start } else { end };
        let to = Vec3::new(push.x + from.x, push.y + from.y, push.z + from.z);
        if let Some((_, p)) = segment_intersect_face(n, from, to, a, b, c) {
            return Some((p, n, dist(p, from)));
        }
    }

    let mut best = NO_HIT;
    let mut out = (Vec3::ZERO, Vec3::ZERO);
    for (p, q) in [(a, b), (b, c), (c, a)] {
        let (hit, on_edge, on_axis, d) = segment_closest_points(p, q, start, end, radius);
        if hit && best > d {
            let v = Vec3::new(on_axis.x - on_edge.x, on_axis.y - on_edge.y, on_axis.z - on_edge.z);
            let len = ((v.x * v.x + v.y * v.y) + v.z * v.z).sqrt();
            let nrm = if len == 0.0 {
                Vec3::ZERO
            } else {
                let inv = 1.0 / len;
                Vec3::new(v.x * inv, v.y * inv, inv * v.z)
            };
            out = (on_edge, nrm);
            best = d;
        }
    }
    (NO_HIT > best).then_some((out.0, out.1, best))
}

#[derive(Default)]
pub(crate) struct TerrainMesh {
    pub(crate) verts: Vec<Vec3>,
    pub(crate) faces: Vec<[usize; 3]>,
}

impl TerrainMesh {
    fn vertex(&mut self, v: Vec3) -> usize {
        let tol = 1.0 / 16384.0;
        if let Some(i) = self.verts.iter().position(|p| tol > (v.x - p.x).abs() && tol > (v.y - p.y).abs() && tol > (v.z - p.z).abs()) {
            return i;
        }
        if self.verts.len() == MAX_MESH_VERTS {
            return MAX_MESH_VERTS - 1;
        }
        self.verts.push(v);
        self.verts.len() - 1
    }

    pub(crate) fn build(ground: &Ground, min: Vec3, max: Vec3) -> Self {
        let mut mesh = Self::default();
        let (x0, x1) = (cvtt(min.x - ORIGIN), cvtt(max.x - ORIGIN));
        let (z0, z1) = (cvtt(min.z - ORIGIN), cvtt(max.z - ORIGIN));
        for iz in z0..=z1 {
            for ix in x0..=x1 {
                if !ground.cell_ok(ix, iz) {
                    continue;
                }
                let p = |dx: i32, dz: i32| Vec3::new((ix + dx) as f32 + ORIGIN, ground.collision_vertex(ix + dx, iz + dz), (iz + dz) as f32 + ORIGIN);
                let (a, b, c, d) = (p(0, 0), p(1, 0), p(1, 1), p(0, 1));
                if mesh.faces.len() > MAX_MESH_FACES - 2 {
                    continue;
                }
                if a.y > min.y || b.y > min.y || c.y > min.y || d.y > min.y {
                    let (ia, ib, ic, id) = (mesh.vertex(a), mesh.vertex(b), mesh.vertex(c), mesh.vertex(d));
                    mesh.faces.push([ia, ib, ic]);
                    mesh.faces.push([ia, ic, id]);
                }
            }
        }
        mesh
    }

    fn intersect(&self, start: Vec3, end: Vec3, radius: f32) -> Option<(Vec3, Vec3, f32)> {
        let mut best: Option<(Vec3, Vec3, f32)> = None;
        let mut best_d = NO_HIT;
        for f in &self.faces {
            let [a, b, c] = f.map(|i| self.verts[i]);
            if let Some(h) = capsule_intersect_triangle(start, end, a, b, c, radius)
                && best_d > h.2
            {
                best_d = h.2;
                best = Some(h);
            }
        }
        best
    }
}

struct Best {
    dist: f32,
    pos: Vec3,
    normal: Vec3,
}

impl Best {
    fn new() -> Self {
        Self { dist: NO_HIT, pos: Vec3::ZERO, normal: Vec3::ZERO }
    }

    fn take(&mut self, h: Option<(Vec3, Vec3, f32)>) -> bool {
        match h {
            Some((p, n, d)) if self.dist > d => {
                *self = Self { dist: d, pos: p, normal: n };
                true
            }
            _ => false,
        }
    }
}

fn cube_faces(start: Vec3, end: Vec3, cell: IVec3, s: f32, v: u32, radius: f32) -> Option<(Vec3, Vec3, f32)> {
    let base = Vec3::new(cell.x as f32 * s, cell.y as f32 * s, cell.z as f32 * s);
    let corner = |i: usize| {
        let c = CUBE_CORNERS[i];
        Vec3::new(c.x * s + base.x, c.y * s + base.y, s * c.z + base.z)
    };
    let mut best = Best::new();
    for r in 0..6 {
        if v & (1 << r) == 0 || v & (64 << r) == 0 {
            continue;
        }
        let [a, b, c, d] = CUBE_FACES[r].map(corner);
        best.take(capsule_intersect_triangle(start, end, a, b, c, radius));
        best.take(capsule_intersect_triangle(start, end, a, c, d, radius));
    }
    (NO_HIT > best.dist).then_some((best.pos, best.normal, best.dist))
}

fn custom_shape(meshes: &BlockMeshes, start: Vec3, end: Vec3, cell: IVec3, s: f32, v: u32, radius: f32) -> Option<(Vec3, Vec3, f32, u32)> {
    let mesh = meshes.get(v & 65535)?;
    let base = Vec3::new(cell.x as f32 * s, cell.y as f32 * s, cell.z as f32 * s);
    let sc = |p: Vec3| Vec3::new(p.x * s + base.x, p.y * s + base.y, s * p.z + base.z);
    let mut best = Best::new();
    let mut attr = 0;
    for (i, q) in mesh.quads.iter().enumerate() {
        let [a, b, c, d] = q.map(|i| sc(mesh.verts[i as usize]));
        if best.take(capsule_intersect_triangle(start, end, a, b, c, radius)) {
            attr = i as u32;
        }
        if best.take(capsule_intersect_triangle(start, end, a, c, d, radius)) {
            attr = i as u32;
        }
    }
    let skip = (v >> 16) & 1023;
    for (i, w) in mesh.walls.iter().enumerate() {
        if skip & (1 << (i & 31)) != 0 {
            continue;
        }
        let [a, b, c, d] = w.map(sc);
        for (p, q, r) in [(a, b, c), (a, c, d), (d, c, b), (d, b, a)] {
            if best.take(capsule_intersect_triangle(start, end, p, q, r, radius)) {
                attr = i as u32 | 0x10000;
            }
        }
    }
    (NO_HIT > best.dist).then_some((best.pos, best.normal, best.dist, attr))
}

pub fn capsule_intersect_area(area: &AreaGrid, meshes: &BlockMeshes, start: Vec3, end: Vec3, radius: f32) -> Option<(Vec3, Vec3, f32, IVec3, u32)> {
    let f = AreaFrame::new(area);
    let (s3, e3, mn, mx) = (start.to_array(), end.to_array(), f.min.to_array(), f.max.to_array());
    for k in 0..3 {
        if mn[k] > radius + s3[k] && mn[k] > radius + e3[k] {
            return None;
        }
    }
    for k in 0..3 {
        if s3[k] - radius > mx[k] && e3[k] - radius > mx[k] {
            return None;
        }
    }
    let (ls, le) = (f.to_local(start), f.to_local(end));
    let (la, lb) = (ls.to_array(), le.to_array());
    let (mut lo, mut hi) = ([0i32; 3], [0i32; 3]);
    for k in 0..3 {
        let small = if lb[k] > la[k] { la[k] } else { lb[k] };
        let big = if la[k] > lb[k] { la[k] } else { lb[k] };
        lo[k] = cvtt((small - radius) * f.inv);
        hi[k] = cvtt((big + radius) * f.inv);
    }

    let mut best = Best::new();
    let mut block = IVec3::ZERO;
    let mut word = 0u32;
    for y in lo[1]..=hi[1] {
        for z in lo[2]..=hi[2] {
            for x in lo[0]..=hi[0] {
                let Some(rec) = area.record(x, y, z) else { continue };
                let cell = IVec3::new(x, y, z);
                let idx = cell_index(x, y, z);
                // TODO: item-set objects (+0x1400, skipping taken ones in +0x1600) and area objects (+0x1200) are tested first
                for layer in 0..2 {
                    let v = if layer == 0 { rec.layer0[idx] } else { rec.layer1[idx] };
                    let shape = |at: IVec3, w: u32| {
                        let h = custom_shape(meshes, ls, le, at, f.size, w, radius)?;
                        area.face_attr.set(h.3);
                        Some((h.0, h.1, h.2))
                    };
                    let (hit, at, w) = match v & TYPE_MASK {
                        CUBE => (cube_faces(ls, le, cell, f.size, v, radius), cell, v),
                        MESH => (shape(cell, v), cell, v),
                        FOOTPRINT => {
                            let o = cell - IVec3::new((v & 255) as i32, ((v >> 16) & 255) as i32, ((v >> 8) & 255) as i32);
                            let v2 = if layer == 0 { area.layer0(o.x, o.y, o.z) } else { area.layer1(o.x, o.y, o.z) };
                            (shape(o, v2), o, v2)
                        }
                        _ => (None, cell, v),
                    };
                    if best.take(hit) {
                        block = at;
                        word = w;
                    }
                }
            }
        }
    }
    if !(NO_HIT > best.dist) {
        return None;
    }
    let p = f.rotate_back(best.pos);
    let pos = Vec3::new(p.x + f.origin.x, p.y + f.origin.y, p.z + f.origin.z);
    Some((pos, f.rotate_back(best.normal), best.dist, block, word))
}

pub fn capsule_intersect_level(ground: &Ground, area: &AreaGrid, meshes: &BlockMeshes, start: Vec3, end: Vec3, radius: f32) -> Option<CapsuleHit> {
    let (smin, smax) = (Vec3::new(start.x - radius, start.y - radius, start.z - radius), Vec3::new(start.x + radius, start.y + radius, start.z + radius));
    let (emin, emax) = (Vec3::new(end.x - radius, end.y - radius, end.z - radius), Vec3::new(end.x + radius, end.y + radius, end.z + radius));
    let (mut min, mut max) = (smin.to_array(), smax.to_array());
    let (emin, emax) = (emin.to_array(), emax.to_array());
    for k in 0..3 {
        if min[k] > emin[k] {
            min[k] = emin[k];
        }
    }
    for k in 0..3 {
        if emin[k] > max[k] {
            max[k] = emin[k];
        }
    }
    for k in 0..3 {
        if min[k] > emax[k] {
            min[k] = emax[k];
        }
    }
    for k in 0..3 {
        if emax[k] > max[k] {
            max[k] = emax[k];
        }
    }

    let mut best: Option<CapsuleHit> = None;
    let mut best_d = NO_HIT;
    let mesh = TerrainMesh::build(ground, Vec3::from_array(min), Vec3::from_array(max));
    if let Some((pos, normal, d)) = mesh.intersect(start, end, radius)
        && best_d > d
    {
        best_d = d;
        best = Some(CapsuleHit { pos, normal, dist: d, area: 0, block: IVec3::splat(-1), cell: 0, face_attr: 0 });
    }
    if let Some((pos, normal, d, block, cell)) = capsule_intersect_area(area, meshes, start, end, radius)
        && best_d > d
    {
        best = Some(CapsuleHit { pos, normal, dist: d, area: 0, block, cell, face_attr: 0 });
    }
    best.map(|h| CapsuleHit { face_attr: area.face_attr.get(), ..h })
}
