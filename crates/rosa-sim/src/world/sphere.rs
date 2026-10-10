use glam::{IVec3, Vec3};

use super::{
    area::{AreaGrid, CUBE, FOOTPRINT, MESH, TYPE_MASK, cell_index},
    capsule::TerrainMesh,
    collide::calculate_face_normal,
    ground::Ground,
    mesh::{CUBE_CORNERS, CUBE_FACES, CUBE_NORMALS},
    meshes::BlockMeshes,
    trace::{AreaFrame, cvtt},
};

const PARALLEL: f32 = 1.0 / 65536.0;
const NO_HIT: f32 = 65536.0;

/// Where a wheel disc touched the level: the contact point, the normal pushing the disc away and the distance from
/// the disc's centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiscHit {
    pub pos: Vec3,
    pub normal: Vec3,
    pub dist: f32,
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

/// segment_intersect_plane_two_sided: where `start..end` crosses the plane through `point` with normal `n`; the
/// fraction along the segment and the point.
pub fn segment_intersect_plane_two_sided(n: Vec3, start: Vec3, end: Vec3, point: Vec3) -> Option<(f32, Vec3)> {
    let pe = sub(point, end);
    let to_end = (pe.x * n.x + pe.y * n.y) + pe.z * n.z;
    let ps = sub(point, start);
    let to_start = (ps.x * n.x + ps.y * n.y) + ps.z * n.z;
    if to_start > 0.0 && to_end > 0.0 {
        return None;
    }
    if 0.0 > to_start && 0.0 > to_end {
        return None;
    }
    let d = sub(end, start);
    let den = (n.x * d.x + n.y * d.y) + n.z * d.z;
    if den == 0.0 {
        return None;
    }
    let t = to_start / den;
    Some((t, Vec3::new(d.x * t + start.x, d.y * t + start.y, t * d.z + start.z)))
}

/// The edge's crossing of the disc's plane, kept when it is nearer the centre than the best so far.
fn disc_edge(best: &mut DiscHit, centre: Vec3, axis: Vec3, a: Vec3, b: Vec3) {
    let Some((_, p)) = segment_intersect_plane_two_sided(axis, a, b, centre) else { return };
    let d = sub(centre, p);
    let dist = (d.z * d.z + (d.x * d.x + d.y * d.y)).sqrt();
    if !(best.dist > dist) {
        return;
    }
    let len = ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt();
    let normal = if len == 0.0 {
        Vec3::ZERO
    } else {
        let inv = 1.0 / len;
        Vec3::new(d.x * inv, d.y * inv, inv * d.z)
    };
    *best = DiscHit { pos: p, normal, dist };
}

/// sphere_intersect_face: a disc of radius `radius` centred on `centre` across `axis` against the triangle `a b c`
/// with normal `normal`: the point of the plane line nearest the centre when it lies inside the triangle, then each
/// edge's crossing of the disc's plane. `hit` starts at the radius; the normal is the face's unless an edge is
/// nearer.
#[allow(clippy::too_many_arguments)]
pub fn sphere_intersect_face(hit: &mut DiscHit, centre: Vec3, axis: Vec3, radius: f32, a: Vec3, b: Vec3, c: Vec3) -> bool {
    hit.dist = radius;
    let n = hit.normal;
    let t = Vec3::new(n.y * axis.z - n.z * axis.y, n.z * axis.x - axis.z * n.x, axis.y * n.x - n.y * axis.x);
    let len2 = (t.y * t.y + t.x * t.x) + t.z * t.z;
    if len2 >= PARALLEL {
        let along = (axis.x * centre.x + axis.y * centre.y) + axis.z * centre.z;
        let plane = (a.x * n.x + a.y * n.y) + a.z * n.z;
        let u = Vec3::new(n.x * along - axis.x * plane, n.y * along - axis.y * plane, n.z * along - axis.z * plane);
        let p = Vec3::new((t.y * u.z - t.z * u.y) / len2, (t.z * u.x - u.z * t.x) / len2, (u.y * t.x - u.x * t.y) / len2);
        let d = sub(centre, p);
        let len = len2.sqrt();
        let dir = if len == 0.0 {
            Vec3::ZERO
        } else {
            let inv = 1.0 / len;
            Vec3::new(t.x * inv, t.y * inv, t.z * inv)
        };
        let proj = (d.x * dir.x + d.y * dir.y) + d.z * dir.z;
        let q = Vec3::new(p.x + dir.x * proj, p.y + dir.y * proj, p.z + proj * dir.z);
        hit.pos = q;
        let e = sub(centre, q);
        let dist = (e.z * e.z + (e.x * e.x + e.y * e.y)).sqrt();
        if radius > dist {
            let side = |w: Vec3, e: Vec3| {
                let c = Vec3::new(w.y * e.z - w.z * e.y, w.z * e.x - e.z * w.x, w.x * e.y - w.y * e.x);
                (c.x * n.x + c.y * n.y) + c.z * n.z
            };
            let last = {
                let (w, e) = (sub(q, c), sub(a, c));
                let k = Vec3::new(w.y * e.z - w.z * e.y, w.z * e.x - e.z * w.x, e.y * w.x - e.x * w.y);
                (k.y * n.y + k.x * n.x) + k.z * n.z
            };
            let inside = side(sub(q, a), sub(b, a)) >= 0.0 && side(sub(q, b), sub(c, b)) >= 0.0 && last >= 0.0;
            if inside {
                hit.dist = dist;
            }
        }
    }
    disc_edge(hit, centre, axis, a, b);
    disc_edge(hit, centre, axis, b, c);
    disc_edge(hit, centre, axis, c, a);
    radius > hit.dist
}

/// sphere_intersect_terrain_mesh: the wheel disc against each landscape cell of `mesh`, both of its triangles under
/// the first triangle's normal (which an edge hit on the first triangle replaces).
fn sphere_intersect_terrain_mesh(mesh: &TerrainMesh, centre: Vec3, axis: Vec3, radius: f32) -> Option<DiscHit> {
    let mut best: Option<DiscHit> = None;
    let mut best_dist = NO_HIT;
    for quad in mesh.faces.chunks(2) {
        let [ia, ib, ic] = quad[0];
        let (a, b, c) = (mesh.verts[ia], mesh.verts[ib], mesh.verts[ic]);
        let mut hit = DiscHit { pos: Vec3::ZERO, normal: calculate_face_normal(a, b, c), dist: radius };
        if sphere_intersect_face(&mut hit, centre, axis, radius, a, b, c) && !(best_dist <= hit.dist) {
            best_dist = hit.dist;
            best = Some(hit);
        }
        if let Some(&[_, _, id]) = quad.get(1)
            && sphere_intersect_face(&mut hit, centre, axis, radius, a, c, mesh.verts[id])
            && !(best_dist <= hit.dist)
        {
            best_dist = hit.dist;
            best = Some(hit);
        }
    }
    best.filter(|_| NO_HIT > best_dist)
}

fn take(best: &mut Option<DiscHit>, best_dist: &mut f32, hit: Option<DiscHit>) {
    if let Some(h) = hit
        && !(*best_dist <= h.dist)
    {
        *best_dist = h.dist;
        *best = Some(h);
    }
}

/// The disc against one triangle under `normal`, when it touches.
#[allow(clippy::too_many_arguments)]
fn face_hit(centre: Vec3, axis: Vec3, radius: f32, normal: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<DiscHit> {
    let mut hit = DiscHit { pos: Vec3::ZERO, normal, dist: radius };
    sphere_intersect_face(&mut hit, centre, axis, radius, a, b, c).then_some(hit)
}

/// sphere_intersect_voxel_cube_with_direction: the disc against the open faces of a cube cell, each face's two
/// triangles under the face's normal.
fn cube_faces(centre: Vec3, axis: Vec3, radius: f32, cell: IVec3, s: f32, v: u32) -> Option<DiscHit> {
    let base = Vec3::new(cell.x as f32 * s, cell.y as f32 * s, cell.z as f32 * s);
    let corner = |i: usize| {
        let c = CUBE_CORNERS[i];
        Vec3::new(c.x * s + base.x, c.y * s + base.y, s * c.z + base.z)
    };
    let (mut best, mut best_dist) = (None, NO_HIT);
    for r in (0..6).filter(|&r| v & (1 << r) != 0 && v & (64 << r) != 0) {
        let [a, b, c, d] = CUBE_FACES[r].map(corner);
        take(&mut best, &mut best_dist, face_hit(centre, axis, radius, CUBE_NORMALS[r], a, b, c));
        take(&mut best, &mut best_dist, face_hit(centre, axis, radius, CUBE_NORMALS[r], a, c, d));
    }
    best
}

/// sphere_intersect_voxel_custom_shape_with_direction: the disc against each quad of a custom shape cell, both
/// triangles under their own normals.
fn custom_shape(meshes: &BlockMeshes, centre: Vec3, axis: Vec3, radius: f32, cell: IVec3, s: f32, v: u32) -> Option<DiscHit> {
    // TODO: the shape's triangle list (+0x60), empty for every shape the level loads, and the face attribute of the hit
    let mesh = meshes.get(v & 65535)?;
    let base = Vec3::new(cell.x as f32 * s, cell.y as f32 * s, cell.z as f32 * s);
    let sc = |p: Vec3| Vec3::new(p.x * s + base.x, p.y * s + base.y, p.z * s + base.z);
    let (mut best, mut best_dist) = (None, NO_HIT);
    for q in &mesh.quads {
        let [a, b, c, d] = q.map(|i| sc(mesh.verts[i as usize]));
        take(&mut best, &mut best_dist, face_hit(centre, axis, radius, calculate_face_normal(a, b, c), a, b, c));
        take(&mut best, &mut best_dist, face_hit(centre, axis, radius, calculate_face_normal(a, c, d), a, c, d));
    }
    best
}

/// sphere_intersect_area: the disc against the cells of a level area it overlaps, the cube and custom shape cells of
/// the first layer and the custom shape cells of the second.
fn sphere_intersect_area(area: &AreaGrid, meshes: &BlockMeshes, centre: Vec3, axis: Vec3, radius: f32) -> Option<DiscHit> {
    let f = AreaFrame::new(area);
    let (c, mn, mx) = (centre.to_array(), f.min.to_array(), f.max.to_array());
    if (0..3).any(|k| mn[k] > c[k] + radius) || (0..3).any(|k| c[k] - radius > mx[k]) {
        return None;
    }
    let local = f.to_local(centre);
    let l = local.to_array();
    let lo: [i32; 3] = std::array::from_fn(|k| cvtt((l[k] - radius) * f.inv));
    let hi: [i32; 3] = std::array::from_fn(|k| cvtt((l[k] + radius) * f.inv));
    let (mut best, mut best_dist) = (None, NO_HIT);
    for y in lo[1]..=hi[1] {
        for z in lo[2]..=hi[2] {
            for x in lo[0]..=hi[0] {
                let Some(rec) = area.record(x, y, z) else { continue };
                let cell = IVec3::new(x, y, z);
                let idx = cell_index(x, y, z);
                let footprint = |v: u32| cell - IVec3::new((v & 255) as i32, ((v >> 16) & 255) as i32, ((v >> 8) & 255) as i32);
                let v = rec.layer0[idx];
                let hit = match v & TYPE_MASK {
                    CUBE => cube_faces(local, axis, radius, cell, f.size, v),
                    MESH => custom_shape(meshes, local, axis, radius, cell, f.size, v),
                    FOOTPRINT => {
                        let o = footprint(v);
                        custom_shape(meshes, local, axis, radius, o, f.size, area.layer0(o.x, o.y, o.z))
                    }
                    _ => None,
                };
                take(&mut best, &mut best_dist, hit);
                let v = rec.layer1[idx];
                let hit = match v & TYPE_MASK {
                    MESH => custom_shape(meshes, local, axis, radius, cell, f.size, v),
                    FOOTPRINT => {
                        let o = footprint(v);
                        custom_shape(meshes, local, axis, radius, o, f.size, area.layer1(o.x, o.y, o.z))
                    }
                    _ => None,
                };
                take(&mut best, &mut best_dist, hit);
            }
        }
    }
    best.map(|h| {
        let p = f.rotate_back(h.pos);
        DiscHit { pos: Vec3::new(p.x + f.origin.x, p.y + f.origin.y, p.z + f.origin.z), normal: f.rotate_back(h.normal), dist: h.dist }
    })
}

/// sphere_cast_objects_and_level: the wheel disc against the landscape around it and against each level area, nearer
/// hits replacing the landscape's.
pub fn sphere_intersect_level(ground: &Ground, area: &AreaGrid, meshes: &BlockMeshes, centre: Vec3, axis: Vec3, radius: f32) -> Option<DiscHit> {
    let min = Vec3::new(centre.x - radius, centre.y - radius, centre.z - radius);
    let max = Vec3::new(centre.x + radius, centre.y + radius, centre.z + radius);
    let mesh = TerrainMesh::build(ground, min, max);
    let mut best_dist = NO_HIT;
    let mut best = None;
    take(&mut best, &mut best_dist, sphere_intersect_terrain_mesh(&mesh, centre, axis, radius));
    take(&mut best, &mut best_dist, sphere_intersect_area(area, meshes, centre, axis, radius));
    best
}

/// segment_intersect_sphere: where the segment from `start` to `end` enters the sphere, as (fraction, point, unit
/// normal); a segment starting inside the sphere does not hit it.
pub fn segment_intersect_sphere(start: Vec3, end: Vec3, center: Vec3, radius: f32) -> Option<(f32, Vec3, Vec3)> {
    let d = sub(end, start);
    let len = ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt();
    let c = sub(center, start);
    let dir = if len == 0.0 {
        Vec3::ZERO
    } else {
        let k = 1.0 / len;
        Vec3::new(d.x * k, d.y * k, k * d.z)
    };
    let t = (dir.x * c.x + dir.y * c.y) + dir.z * c.z;
    if 0.0 > t {
        return None;
    }
    let c2 = c.z * c.z + (c.y * c.y + c.x * c.x);
    let d2 = c2 - t * t;
    let r2 = radius * radius;
    if d2 > r2 || r2 > c2 {
        return None;
    }
    let h = (r2 - d2).sqrt();
    let frac = (t - h) / len;
    if frac > 1.0 {
        return None;
    }
    let p = Vec3::new((end.x - start.x) * frac + start.x, (end.y - start.y) * frac + start.y, frac * (end.z - start.z) + start.z);
    let n = sub(p, center);
    let nl = ((n.x * n.x + n.y * n.y) + n.z * n.z).sqrt();
    let n = if nl == 0.0 {
        Vec3::ZERO
    } else {
        let k = 1.0 / nl;
        Vec3::new(n.x * k, n.y * k, k * n.z)
    };
    Some((frac, p, n))
}

/// sphere_intersect_mesh: the wheel disc against each of the triangles under its own normal, the nearest hit.
pub fn sphere_intersect_triangles(tris: &[crate::world::track::TrackTriangle], centre: Vec3, axis: Vec3, radius: f32) -> Option<DiscHit> {
    let (mut best, mut best_dist) = (None, NO_HIT);
    for &[a, b, c] in tris {
        take(&mut best, &mut best_dist, face_hit(centre, axis, radius, calculate_face_normal(a, b, c), a, b, c));
    }
    best.filter(|_| NO_HIT > best_dist)
}
