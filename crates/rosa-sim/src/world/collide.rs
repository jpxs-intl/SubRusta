use glam::Vec3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceHit {
    pub fraction: f32,
    pub pos: Vec3,
    pub normal: Vec3,
}

pub fn calculate_face_normal(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    let (e1x, e1y, e1z) = (b.x - a.x, b.y - a.y, b.z - a.z);
    let (e2x, e2y, e2z) = (c.x - a.x, c.y - a.y, c.z - a.z);

    let nx = e2y * e1z - e2z * e1y;
    let ny = e2z * e1x - e1z * e2x;
    let nz = e2x * e1y - e2y * e1x;

    let len = (nz * nz + (nx * nx + ny * ny)).sqrt();
    if len == 0.0 {
        return Vec3::ZERO;
    }

    let inv = 1.0 / len;

    Vec3::new(nx * inv, ny * inv, nz * inv)
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

    let (dx, dy, dz) = (end.x - start.x, end.y - start.y, end.z - start.z);
    let denom = n.x * dx + n.y * dy + n.z * dz;

    if denom == 0.0 {
        return None;
    }

    let t = d_start / denom;

    Some((t, Vec3::new(dx * t + start.x, dy * t + start.y, t * dz + start.z)))
}

#[inline]
fn edge(p: Vec3, v0: Vec3, v1: Vec3, n: Vec3) -> f32 {
    let (ex, ey, ez) = (v1.x - v0.x, v1.y - v0.y, v1.z - v0.z);
    let (px, py, pz) = (p.x - v0.x, p.y - v0.y, p.z - v0.z);

    let cx = ez * py - ey * pz;
    let cy = pz * ex - ez * px;
    let cz = ey * px - ex * py;
    
    cx * n.x + cy * n.y + cz * n.z
}

pub fn point_in_triangle_oriented(p: Vec3, n: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
    edge(p, a, b, n) >= 0.0 && edge(p, b, c, n) >= 0.0 && edge(p, c, a, n) >= 0.0
}

pub fn segment_intersect_face(n: Vec3, start: Vec3, end: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<(f32, Vec3)> {
    let (t, p) = segment_intersect_plane_one_sided(n, a, start, end)?;
    point_in_triangle_oriented(p, n, a, b, c).then_some((t, p))
}
