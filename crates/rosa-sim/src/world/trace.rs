use glam::{IVec3, Vec3};

use crate::world::{
    area::{AreaGrid, CUBE, FOOTPRINT, MESH, TYPE_MASK, cell_index},
    city_objects::{object_pose, segment_intersect_object},
    collide::{TraceHit, calculate_face_normal, segment_intersect_face},
    ground::Ground,
    mesh::{CUBE_CORNERS, CUBE_FACES, CUBE_NORMALS},
    meshes::BlockMeshes,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelHit {
    pub hit: TraceHit,
    pub area: i32,
    pub block: IVec3,
    /// The area word of the cell hit (a footprint's origin cell word), and the face hit within a custom shape (its
    /// quad index, or wall index | 0x10000).
    pub cell: u32,
    pub face_attr: u32,
    /// The area object type the hit was on (line_intersect_result.unk23), -1 for anything else.
    pub object: i32,
}

#[inline]
pub(crate) fn cvtt(v: f32) -> i32 {
    if (-2147483648.0..2147483648.0).contains(&v) { v as i32 } else { i32::MIN }
}

pub(crate) struct AreaFrame {
    pub(crate) origin: Vec3,
    m: [f32; 9],
    pub(crate) size: f32,
    pub(crate) inv: f32,
    pub(crate) min: Vec3,
    pub(crate) max: Vec3,
}

impl AreaFrame {
    pub(crate) fn new(a: &AreaGrid) -> Self {
        let o = a.origin;
        let s = a.block_size;
        let c = a.chunks;
        Self {
            origin: o,
            m: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            size: s,
            inv: 1.0 / s,
            min: o,
            max: Vec3::new(
                c.x as f32 * 8.0 * 8.0 * s + o.x,
                c.y as f32 * 8.0 * 8.0 * s + o.y,
                s * c.z as f32 * 8.0 * 8.0 + o.z,
            ),
        }
    }

    pub(crate) fn to_local(&self, p: Vec3) -> Vec3 {
        let m = &self.m;
        let (dx, dy, dz) = (p.x - self.origin.x, p.y - self.origin.y, p.z - self.origin.z);
        Vec3::new(m[0] * dx + m[1] * dy + m[2] * dz, m[3] * dx + m[4] * dy + m[5] * dz, dx * m[6] + dy * m[7] + dz * m[8])
    }

    pub(crate) fn rotate_back(&self, v: Vec3) -> Vec3 {
        let m = &self.m;
        Vec3::new(m[0] * v.x + m[3] * v.y + m[6] * v.z, m[1] * v.x + m[4] * v.y + m[7] * v.z, m[2] * v.x + m[5] * v.y + m[8] * v.z)
    }
}

fn masked_cell_faces(start: Vec3, end: Vec3, cell: IVec3, s: f32, v: u32) -> Option<(f32, Vec3, Vec3)> {
    let base = Vec3::new(cell.x as f32 * s, cell.y as f32 * s, cell.z as f32 * s);
    let corner = |i: usize| {
        let c = CUBE_CORNERS[i];
        Vec3::new(c.x * s + base.x, c.y * s + base.y, s * c.z + base.z)
    };
    for r in 0..6 {
        if v & (1 << r) == 0 || v & (64 << r) == 0 {
            continue;
        }
        let f = CUBE_FACES[r];
        let n = CUBE_NORMALS[r];
        let a = corner(f[0]);
        if let Some((t, p)) = segment_intersect_face(n, start, end, a, corner(f[1]), corner(f[2])) {
            return Some((t, p, n));
        }
        if let Some((t, p)) = segment_intersect_face(n, start, end, a, corner(f[2]), corner(f[3])) {
            return Some((t, p, n));
        }
    }
    None
}

fn scaled_cell_mesh(meshes: &BlockMeshes, start: Vec3, end: Vec3, cell: IVec3, s: f32, v: u32) -> Option<(f32, Vec3, Vec3, u32)> {
    let mesh = meshes.get(v & 65535)?;
    let base = Vec3::new(cell.x as f32 * s, cell.y as f32 * s, cell.z as f32 * s);
    let sc = |p: Vec3| Vec3::new(p.x * s + base.x, p.y * s + base.y, s * p.z + base.z);
    let mut best = 1.0f32;
    let mut out = (Vec3::ZERO, Vec3::ZERO, 0u32);
    let take = |n: Vec3, a: Vec3, b: Vec3, c: Vec3, attr: u32, best: &mut f32, out: &mut (Vec3, Vec3, u32)| {
        if let Some((t, p)) = segment_intersect_face(n, start, end, a, b, c) && !(*best <= t) {
                *best = t;
                *out = (p, n, attr);
            }
    };
    for (i, q) in mesh.quads.iter().enumerate() {
        let [a, b, c, d] = q.map(|i| sc(mesh.verts[i as usize]));
        let attr = i as u32;
        take(calculate_face_normal(a, b, c), a, b, c, attr, &mut best, &mut out);
        take(calculate_face_normal(a, c, d), a, c, d, attr, &mut best, &mut out);
    }
    let skip = (v >> 16) & 1023;
    for (i, w) in mesh.walls.iter().enumerate() {
        if skip & (1 << (i & 31)) != 0 {
            continue;
        }
        let [a, b, c, d] = w.map(sc);
        let attr = i as u32 | 0x10000;
        let n = calculate_face_normal(a, b, c);
        take(n, a, b, c, attr, &mut best, &mut out);
        take(n, a, c, d, attr, &mut best, &mut out);
        let n = calculate_face_normal(a, c, b);
        take(n, a, c, b, attr, &mut best, &mut out);
        take(n, a, d, c, attr, &mut best, &mut out);
    }
    (1.0 > best).then_some((best, out.0, out.1, out.2))
}

pub fn line_intersect_area(area: &AreaGrid, meshes: &BlockMeshes, start: Vec3, end: Vec3) -> Option<(TraceHit, IVec3, u32, u32, i32)> {
    let f = AreaFrame::new(area);
    let (s3, e3) = (start.to_array(), end.to_array());
    let (mn, mx) = (f.min.to_array(), f.max.to_array());
    for k in 0..3 {
        if mn[k] > s3[k] && mn[k] > e3[k] {
            return None;
        }
    }
    for k in 0..3 {
        if s3[k] > mx[k] && e3[k] > mx[k] {
            return None;
        }
    }
    let ls = f.to_local(start).to_array();
    let le = f.to_local(end).to_array();
    let inv = f.inv;
    let mut c = [0i32; 3];
    let mut c1 = [0i32; 3];
    let mut step = [0i32; 3];
    let mut d = [0f32; 3];
    for k in 0..3 {
        c[k] = cvtt(inv * ls[k]);
        c1[k] = cvtt(inv * le[k]);
        let dd = (le[k] - ls[k]) * inv;
        step[k] = if dd > 0.0 { 1 } else { -1 };
        d[k] = dd.abs();
    }
    let mut a = d;
    let mut b = d;
    let mut count = 0i32;
    if c != c1 {
        for k in 0..3 {
            let v = ls[k] * inv;
            let mut fr = (v as f64 - (v.floor() as f64 + 0.5)) as f32;
            if step[k] > 0 {
                fr = -fr;
            }
            let f2 = fr + fr;
            match k {
                0 => {
                    a[0] = d[1] * f2 - a[0];
                    b[0] = f2 * d[2] - b[0];
                }
                1 => {
                    a[1] = d[0] * f2 - a[1];
                    b[1] = f2 * d[2] - b[1];
                }
                _ => {
                    a[2] = d[0] * f2 - a[2];
                    b[2] = f2 * d[1] - b[2];
                }
            }
        }
        count = (c1[0] - c[0]).wrapping_abs() + (c1[1] - c[1]).wrapping_abs() + (c1[2] - c[2]).wrapping_abs();
        count = count.min(16384);
    }

    let mut best = 1.0f32;
    let mut result: Option<(TraceHit, IVec3, u32, u32, i32)> = None;
    let (sl, el) = (Vec3::from_array(ls), Vec3::from_array(le));
    let mut n = 0;
    loop {
        let cell = IVec3::from_array(c);
        if let Some(rec) = area.record(cell.x, cell.y, cell.z) {
            let idx = cell_index(cell.x, cell.y, cell.z);
            // TODO: item sets (+0x1400) are tested first in the binary
            let object = rec.object[idx];
            if object as i32 > 0 {
                let (kind, pos, rot) = object_pose(object, cell, f.size);
                if let Some((t, p, nrm)) = segment_intersect_object(kind, pos, &rot, sl, el)
                    && best > t
                {
                    best = t;
                    let p = f.rotate_back(p);
                    let pos = Vec3::new(p.x + f.origin.x, p.y + f.origin.y, p.z + f.origin.z);
                    result = Some((TraceHit { fraction: t, pos, normal: f.rotate_back(nrm) }, cell, 0, 0, kind as i32));
                }
            }
            for layer in 0..2 {
                let v = if layer == 0 { rec.layer0[idx] } else { rec.layer1[idx] };
                if v == 0 {
                    continue;
                }
                let (hit, at, w) = match v & TYPE_MASK {
                    CUBE => (masked_cell_faces(sl, el, cell, f.size, v).map(|(t, p, n)| (t, p, n, 0)), cell, v),
                    MESH => (scaled_cell_mesh(meshes, sl, el, cell, f.size, v), cell, v),
                    FOOTPRINT => {
                        let o = cell - IVec3::new((v & 255) as i32, ((v >> 16) & 255) as i32, ((v >> 8) & 255) as i32);
                        let v2 = if layer == 0 { area.layer0(o.x, o.y, o.z) } else { area.layer1(o.x, o.y, o.z) };
                        (scaled_cell_mesh(meshes, sl, el, o, f.size, v2), o, v2)
                    }
                    _ => (None, cell, v),
                };
                if let Some((t, p, nrm, attr)) = hit && !(best <= t) {
                        best = t;
                        let p = f.rotate_back(p);
                        let pos = Vec3::new(p.x + f.origin.x, p.y + f.origin.y, p.z + f.origin.z);
                        result = Some((TraceHit { fraction: t, pos, normal: f.rotate_back(nrm) }, at, w, attr, -1));
                    }
            }
            if 1.0 > best {
                return result;
            }
        }
        if c == c1 {
            return None;
        }
        if !(a[0] >= a[1]) && !(b[0] >= a[2]) {
            c[0] += step[0];
            a[1] -= d[1];
            a[0] += d[1];
            a[2] -= d[2];
            b[0] += d[2];
        } else if b[2] >= b[1] {
            c[1] += step[1];
            a[1] += d[0];
            a[0] -= d[0];
            b[1] += d[2];
            b[2] -= d[2];
        } else {
            c[2] += step[2];
            a[2] += d[0];
            b[0] -= d[0];
            b[1] -= d[1];
            b[2] += d[1];
        }
        n += 1;
        if count < n {
            return None;
        }
    }
}

pub fn line_intersect_level(ground: &Ground, area: &AreaGrid, meshes: &BlockMeshes, start: Vec3, end: Vec3) -> Option<LevelHit> {
    let mut best: Option<LevelHit> = None;
    let mut frac = 1.0f32;
    if let Some(h) = ground.line_intersect_landscape(start, end) && frac > h.fraction {
            frac = h.fraction;
            best = Some(LevelHit { hit: h, area: -1, block: IVec3::splat(-1), cell: 0, face_attr: 0, object: -1 });
        }
    if let Some((h, block, cell, face_attr, object)) = line_intersect_area(area, meshes, start, end) {
        if !(frac <= h.fraction) {
            frac = h.fraction;
            best = Some(LevelHit { hit: h, area: 0, block, cell, face_attr, object });
        } else if let Some(b) = &mut best {
            b.area = 0;
            b.block = block;
            b.cell = cell;
            b.face_attr = face_attr;
            b.object = object;
        }
    }
    if 1.0 > frac { best } else { None }
}
