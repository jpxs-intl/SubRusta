use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceHit {
    pub fraction: f32,
    pub pos: Vec3,
    pub normal: Vec3,
}

pub fn calculate_face_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let e1 = b - a;
    let e2 = c - a;

    let n = e2.cross(e1);

    let len = n.length();
    if len == 0.0 {
        return Vec3::ZERO;
    }

    let inv = 1.0 / len;

    n * inv
}

#[inline]
fn dot_sub(p: Vec3, q: Vec3, n: Vec3) -> f32 {
    (p.x - q.x) * n.x + (p.y - q.y) * n.y + (p.z - q.z) * n.z
}

pub fn segment_intersect_plane_one_sided(n: Vec3, p: Vec3, start: Vec3, end: Vec3) -> Option<(f32, Vec3)> {
    if dot_sub(p, end, n) < 0.0 {
        return None;
    }

    let d_start = dot_sub(p, start, n);
    if d_start > 0.0 {
        return None;
    }

    let d = end - start;
    let denom = n.dot(d);

    if denom == 0.0 {
        return None;
    }

    let t = d_start / denom;

    Some((t, d * t + start))
}

#[inline]
fn edge(p: Vec3, v0: Vec3, v1: Vec3, n: Vec3) -> f32 {
    let e = v1 - v0;
    let (px, py, pz) = (p.x - v0.x, p.y - v0.y, p.z - v0.z);

    let cx = e.z * py - e.y * pz;
    let cy = pz * e.x - e.z * px;
    let cz = e.y * px - e.x * py;

    cx * n.x + cy * n.y + cz * n.z
}

pub fn point_in_triangle_oriented(p: Vec3, n: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
    edge(p, a, b, n) >= 0.0 && edge(p, b, c, n) >= 0.0 && edge(p, c, a, n) >= 0.0
}

pub fn segment_intersect_face(n: Vec3, start: Vec3, end: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<(f32, Vec3)> {
    let (t, p) = segment_intersect_plane_one_sided(n, a, start, end)?;
    point_in_triangle_oriented(p, n, a, b, c).then_some((t, p))
}
