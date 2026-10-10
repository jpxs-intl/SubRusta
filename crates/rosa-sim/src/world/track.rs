use glam::Vec3;
use rosa_physics::rotation::rotate_vector_about_axis;

use crate::world::{collide::calculate_face_normal, trace::cvtt};

/// The track grid: 64 by 64 cells of 64 units, each listing up to 8 meshes.
pub const GRID_CELLS: usize = 64;
const CELL_SCALE: f32 = 1.0 / 64.0;
const CELL_MESHES: usize = 8;
const BOX_INIT: f32 = 16777216.0;

const AXIS: Vec3 = Vec3::Y;
const TUBE: i32 = 0;
const QUAD: i32 = 1;
const TUBE_RADIUS: f32 = f32::from_bits(0x3fb7_ae14);
const TUBE_RINGS: i32 = 16;
const RING_STEP: f32 = f32::from_bits(0x3d88_8889);
const RAIL_LIFT: f32 = 0.5;
const BED_DROP: f32 = 2.0;
const BED_OUTER: f32 = -6.0;
const BED_INNER: f32 = -5.0;
const THIRD: f32 = f32::from_bits(0x3eaa_aaab);
const PATCH_STEPS: usize = 17;
const PATCH_STEP: f32 = 0.0625;
const PATCH_EDGE_HEIGHT: f32 = 23.0;

/// The race track: 104 tube pieces from the start, each 56 or 400/7 long, turning and sloping at set pieces.
const TRACK_START: Vec3 = Vec3::new(2480.0, 36.0625, 1021.25);
const TRACK_PIECES: i32 = 103;
const FIRST_PIECE_HALF: f32 = 28.0;
const STRAIGHT: f32 = 56.0;
const LONG: f32 = f32::from_bits(0x4264_9249);
const TURN: f32 = -22.5_f32.to_radians();
const SLOPE: f32 = 7.0 / 128.0;
const ARC: f32 = 114.75;
const SPAWN_SOUTH: f32 = 270.0_f32.to_radians();
const SPAWN_EAST: f32 = 90.0_f32.to_radians();
/// The curved ramps: a 32 by 64 slab, then 21 pieces bending a quarter turn a piece around 64 radius.
const RAMP_CORNERS: [[u32; 3]; 4] = [[0x453f_0000, 0x41c0_8000, 0x4494_0000], [0x4541_0000, 0x41c0_8000, 0x4494_0000], [0x4541_0000, 0x41c0_8000, 0x449c_0000], [0x453f_0000, 0x41c0_8000, 0x449c_0000]];
const RAMP_PIECES: i32 = 21;
const RAMP_TURN: f64 = 45.0_f64.to_radians();
const RAMP_RADIUS: f32 = 64.0;
const RAMP_HALF_WIDTH: f32 = 16.0;

/// A dynamic level mesh (0x462d7dc0 + 0xf120 each): a piece of the race track or a ramp.
#[derive(Clone, Debug, Default)]
pub struct TrackMesh {
    pub kind: i32,
    pub center: Vec3,
    pub verts: Vec<Vec3>,
    pub tris: Vec<[i32; 3]>,
    /// The triangle ranges of the mesh's parts: starts at 0..3, ends at 4..7 (a tube's rails, bed and walls).
    pub pieces: [i32; 8],
    pub normals: Vec<Vec3>,
    pub tri_boxes: Vec<[Vec3; 2]>,
    pub box_min: Vec3,
    pub box_max: Vec3,
    /// The track pieces before and after this one.
    pub prev: i32,
    pub next: i32,
    /// How straight the piece is: its end directions' dot product to the fourth, then eased with its neighbours'.
    pub weight: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Track {
    pub meshes: Vec<TrackMesh>,
    pub grid: Vec<Vec<i32>>,
    /// Where trains are put (0x1d2c1280): position and facing.
    pub spawns: Vec<(Vec3, f32)>,
}

fn bezier(s: f32, p1: f32, p2: f32, e: f32, u: f32, t: f32) -> f32 {
    ((s * u * u * u + p1 * 3.0 * u * u * t) + p2 * 3.0 * u * t * t) + e * t * t * t
}

fn bezier3(s: Vec3, p1: Vec3, p2: Vec3, e: Vec3, u: f32, t: f32) -> Vec3 {
    Vec3::new(bezier(s.x, p1.x, p2.x, e.x, u, t), bezier(s.y, p1.y, p2.y, e.y, u, t), bezier(s.z, p1.z, p2.z, e.z, u, t))
}

fn bern(a: f32, b: f32, c: f32, d: f32, w: f32, v: f32) -> f32 {
    ((a * w * w * w + b * 3.0 * w * w * v) + c * 3.0 * w * v * v) + d * v * v * v
}

fn bern3(a: Vec3, b: Vec3, c: Vec3, d: Vec3, w: f32, v: f32) -> Vec3 {
    Vec3::new(bern(a.x, b.x, c.x, d.x, w, v), bern(a.y, b.y, c.y, d.y, w, v), bern(a.z, b.z, c.z, d.z, w, v))
}

/// The unit side vector of a direction: direction × up, both normalised (zero when either is zero).
fn side_of(d: Vec3) -> Option<Vec3> {
    let l = ((d.y * d.y + d.x * d.x) + d.z * d.z).sqrt();
    let n = if l == 0.0 {
        Vec3::ZERO
    } else {
        let inv = 1.0 / l;
        Vec3::new(d.x * inv, d.y * inv, d.z * inv)
    };
    let (ax, ay, az) = (AXIS.x, AXIS.y, AXIS.z);
    let s = Vec3::new(n.y * az - n.z * ay, n.z * ax - az * n.x, n.x * ay - n.y * ax);
    let ls = ((s.y * s.y + s.x * s.x) + s.z * s.z).sqrt();
    if ls == 0.0 {
        return None;
    }
    let inv = 1.0 / ls;
    Some(Vec3::new(s.x * inv, s.y * inv, inv * s.z))
}

fn horizontal(dx: f32, dz: f32) -> f32 {
    ((dx * dx + 0.0) + dz * dz).sqrt()
}

impl Track {
    /// level_generate_race_track: the tube track with its train spawns, the pieces linked in a loop and their
    /// weights eased, then the ramps (build_roundcity_curved_meshes).
    pub fn race_track() -> Self {
        let mut t = Track { meshes: Vec::new(), grid: vec![Vec::new(); GRID_CELLS * GRID_CELLS], spawns: Vec::new() };
        let v = Vec3::new(-1.0, -0.0, -0.0);
        let s = TRACK_START;
        let (a, b, c) = (v.x * FIRST_PIECE_HALF, v.y * FIRST_PIECE_HALF, v.z * FIRST_PIECE_HALF);
        let e = Vec3::new((s.x + a) + a, (s.y + b) + b, (s.z + c) + c);
        let (mut d0, mut d1) = (v, v);
        t.tube(TUBE, s, e, d0, d1, TUBE_RADIUS);
        t.spawns.push((Vec3::new((s.x + e.x) * 0.5, (s.y + e.y) * 0.5, (s.z + e.z) * 0.5), SPAWN_SOUTH));
        let (mut angle, mut ydir) = (0.0f32, 0.0f32);
        let mut end = e;
        let mut prev = 0usize;
        for k in 0..TRACK_PIECES {
            let len = if (23..=40).contains(&k) || (75..=92).contains(&k) { LONG } else { STRAIGHT };
            let start = end;
            d0 = d1;
            let mut generic = true;
            match k {
                23 | 41 | 75 => angle = TURN,
                27 | 45 | 79 => angle = 0.0,
                70 => ydir = -SLOPE,
                74 => ydir = 0.0,
                93 => {
                    d1 = rotate_vector_about_axis(d1, AXIS, TURN);
                    d1.y = ydir;
                    angle = TURN;
                    generic = false;
                }
                97 => {
                    d1 = rotate_vector_about_axis(d1, AXIS, 0.0);
                    angle = 0.0;
                    d1.y = ydir;
                    generic = false;
                }
                98 => {
                    d1 = rotate_vector_about_axis(d1, AXIS, angle);
                    d1.y = SLOPE;
                    ydir = SLOPE;
                    generic = false;
                }
                102 => {
                    d1 = rotate_vector_about_axis(d1, AXIS, angle);
                    d1.y = 0.0;
                    ydir = 0.0;
                    generic = false;
                }
                _ => {}
            }
            if generic {
                d1 = rotate_vector_about_axis(d1, AXIS, angle);
                d1.y = ydir;
                match k {
                    19 => t.spawns.push((end, SPAWN_SOUTH)),
                    49 | 69 => t.spawns.push((end, SPAWN_EAST)),
                    86 => t.spawns.push((end, 0.0)),
                    _ => {}
                }
            }
            let h = 0.5 * len;
            end = Vec3::new((h * d0.x + start.x) + h * d1.x, (h * d0.y + start.y) + ydir * h, h * d1.z + (h * d0.z + start.z));
            if angle != 0.0 {
                let (ax, ay, az) = (AXIS.x, AXIS.y, AXIS.z);
                let c0 = Vec3::new(d0.y * az - d0.z * ay, d0.z * ax - d0.x * az, d0.x * ay - d0.y * ax);
                let c1 = Vec3::new(ydir * az - d1.z * ay, d1.z * ax - az * d1.x, d1.x * ay - ax * ydir);
                let (k0, k1) = if angle > 0.0 { (ARC, -ARC) } else { (-ARC, ARC) };
                end = Vec3::new(c1.x * k1 + (start.x + c0.x * k0), c1.y * k1 + (start.y + c0.y * k0), c1.z * k1 + (start.z + c0.z * k0));
            }
            let id = t.tube(TUBE, start, end, d0, d1, TUBE_RADIUS);
            t.meshes[id].prev = prev as i32;
            t.meshes[prev].next = id as i32;
            prev = id;
        }
        t.meshes[0].prev = prev as i32;
        t.meshes[prev].next = 0;
        for _ in 0..2 {
            for m in 0..t.meshes.len() {
                let (p, n) = (t.meshes[m].prev as usize, t.meshes[m].next as usize);
                let a = minss((t.meshes[p].weight + 1.0) * 0.5, t.meshes[m].weight);
                let b = (t.meshes[n].weight + 1.0) * 0.5;
                t.meshes[m].weight = minss(b, a);
            }
        }
        t.ramps();
        t
    }

    /// build_roundcity_curved_meshes.
    fn ramps(&mut self) {
        let mut c = RAMP_CORNERS.map(|p| Vec3::from_array(p.map(f32::from_bits)));
        self.quad(QUAD, c, 0.0, 0.0);
        let mut mid = Vec3::new((c[2].x + c[3].x) * 0.5, (c[2].y + c[3].y) * 0.5, (c[2].z + c[3].z) * 0.5);
        let mut a = 0.0f32;
        for k in 0..RAMP_PIECES {
            let old = a;
            if k <= 4 || (10..=13).contains(&k) || k == 17 {
                a = ((a as f64) + RAMP_TURN) as f32;
            } else if k == 5 || k == 6 {
                a = ((a as f64) - RAMP_TURN) as f32;
            }
            c[0] = c[3];
            c[1] = c[2];
            let half = 0.5 * a + 0.5 * old;
            let (s, co) = (half as f64).sin_cos();
            mid = Vec3::new((s as f32) * RAMP_RADIUS + mid.x, 0.0 + mid.y, (co as f32) * RAMP_RADIUS + mid.z);
            let (s2, c2) = (a as f64).sin_cos();
            let (s2, c2) = (s2 as f32, c2 as f32);
            c[2] = Vec3::new(RAMP_HALF_WIDTH * c2 + mid.x, 0.0 + mid.y, mid.z - RAMP_HALF_WIDTH * s2);
            c[3] = Vec3::new(c2 * -RAMP_HALF_WIDTH + mid.x, mid.y, s2 * RAMP_HALF_WIDTH + mid.z);
            if k <= 3 || (10..=12).contains(&k) {
                c[3].y += 16.0;
            }
            if k == 5 {
                c[2].y += 16.0;
            }
            if k == 17 {
                c[3].y += 8.0;
            }
            if k == 19 {
                c[2].y += 4.0;
                c[3].y += 4.0;
            }
            self.quad(QUAD, c, 0.0, 0.0);
        }
    }

    /// dynamic_mesh_build_tube: a piece of track along the cubic from `s` to `e` leaving along `d0` and arriving
    /// along `d1`: two rails `radius` apart, then the bed below them, 16 rings each.
    fn tube(&mut self, kind: i32, s: Vec3, e: Vec3, d0: Vec3, d1: Vec3, radius: f32) -> usize {
        let mut m = TrackMesh { kind, ..Default::default() };
        m.center = Vec3::new((e.x + s.x) * 0.5, (e.y + s.y) * 0.5, (e.z + s.z) * 0.5);
        let dot = (d0.x * d1.x + d0.y * d1.y) + d0.z * d1.z;
        let sq = dot * dot;
        m.weight = sq * sq;
        let (dx, dy, dz) = (e.x - s.x, e.y - s.y, e.z - s.z);
        let l = ((dx * dx + dy * dy) + dz * dz).sqrt();
        let t3 = l / 3.0;
        let p1 = Vec3::new(t3 * d0.x + s.x, t3 * d0.y + s.y, t3 * d0.z + s.z);
        let m3 = (-l) / 3.0;
        let p2 = Vec3::new(m3 * d1.x + e.x, m3 * d1.y + e.y, m3 * d1.z + e.z);
        let dir = |t: f32| {
            let u = 1.0 - t;
            Vec3::new(d0.x * u + d1.x * t, d1.y * t + d0.y * u, d1.z * t + u * d0.z)
        };
        let (mut t, mut p) = (0.0f32, s);
        for ring in 1..=TUBE_RINGS {
            let sd = side_of(dir(t)).unwrap_or(Vec3::ZERO);
            let a = (-radius) * 0.5;
            let v0 = Vec3::new(a * sd.x + p.x, a * sd.y + p.y, a * sd.z + p.z);
            let b = radius * 0.5;
            let v2 = Vec3::new(sd.x * b + p.x, sd.y * b + p.y, sd.z * b + p.z);
            m.verts.extend([Vec3::new(v0.x, v0.y + RAIL_LIFT, v0.z), v0, v2, Vec3::new(v2.x, v2.y + RAIL_LIFT, v2.z)]);
            t += RING_STEP;
            if ring == TUBE_RINGS {
                break;
            }
            p = if ring == TUBE_RINGS - 1 { e } else { bezier3(s, p1, p2, e, 1.0 - t, t) };
        }
        let (mut t, mut p) = (0.0f32, s);
        for ring in 1..=TUBE_RINGS {
            let d = dir(t);
            let quad = match side_of(Vec3::new(d.x, d.y, d.z)) {
                None => [p, Vec3::new(0.0 + p.x, 0.0 + p.y, 0.0 + p.z), Vec3::new(p.x + 0.0, 0.0 + p.y, 0.0 + p.z), p],
                Some(sd) => [
                    Vec3::new(BED_OUTER * sd.x + p.x, BED_OUTER * sd.y + p.y, BED_OUTER * sd.z + p.z),
                    Vec3::new((sd.x + sd.x) + p.x, (sd.y + sd.y) + p.y, (sd.z + sd.z) + p.z),
                    Vec3::new(p.x + sd.x, sd.y + p.y, sd.z + p.z),
                    Vec3::new(BED_INNER * sd.x + p.x, BED_INNER * sd.y + p.y, BED_INNER * sd.z + p.z),
                ],
            };
            m.verts.extend([quad[0], quad[1], Vec3::new(quad[2].x, quad[2].y - BED_DROP, quad[2].z), Vec3::new(quad[3].x, quad[3].y - BED_DROP, quad[3].z)]);
            t += RING_STEP;
            if ring == TUBE_RINGS {
                break;
            }
            p = if ring == TUBE_RINGS - 1 { e } else { bezier3(s, p1, p2, e, 1.0 - t, t) };
        }
        let rails = (0..15).flat_map(|k| {
            let a = 1 + 4 * k;
            [[a, a + 4, a + 5], [a, a + 5, a + 1]]
        });
        m.tris.extend(rails);
        m.pieces[4] = m.tris.len() as i32;
        m.pieces[1] = m.tris.len() as i32;
        let bed = (0..15).flat_map(|k| {
            let a = 4 * k;
            [[a, a + 4, a + 5], [a, a + 5, a + 1], [a + 2, a + 6, a + 7], [a + 2, a + 7, a + 3]]
        });
        m.tris.extend(bed);
        m.pieces[5] = m.tris.len() as i32;
        m.pieces[2] = m.tris.len() as i32;
        let walls = (0..15).flat_map(|k| {
            let d = 64 + 4 * k;
            [[d, d + 4, d + 5], [d, d + 5, d + 1], [d + 3, d + 7, d + 4], [d + 3, d + 4, d], [d + 1, d + 5, d + 6], [d + 1, d + 6, d + 2], [d + 2, d + 6, d + 7], [d + 2, d + 7, d + 3]]
        });
        m.tris.extend(walls);
        m.pieces[6] = m.tris.len() as i32;
        self.add(m)
    }

    /// dynamic_mesh_build_subdivided_quad: a bicubic patch over the four corners, its sides bulging along the
    /// ground by a third of their length, raised by `e1` and lowered by `e0` over thirds; 17 by 17 points with the
    /// side columns at height 23.
    fn quad(&mut self, kind: i32, c: [Vec3; 4], e0: f32, e1: f32) -> usize {
        let mut m = TrackMesh { kind, ..Default::default() };
        m.center = Vec3::new((((c[0].x + c[1].x) + c[2].x) + c[3].x) * 0.25, (((c[0].y + c[1].y) + c[2].y) + c[3].y) * 0.25, (((c[0].z + c[1].z) + c[2].z) + c[3].z) * 0.25);
        let perp01 = {
            let (dx, dz) = (c[0].x - c[1].x, c[0].z - c[1].z);
            let l = ((dz * dz + 0.0) + dx * dx).sqrt();
            if l == 0.0 {
                Vec3::ZERO
            } else {
                let inv = 1.0 / l;
                Vec3::new(dz * inv, inv * 0.0, (-dx) * inv)
            }
        };
        let e1d = e1 / 3.0;
        let t03 = horizontal(c[3].x - c[0].x, c[3].z - c[0].z) / 3.0;
        let r1p0 = Vec3::new(perp01.x * t03 + c[0].x, (t03 * perp01.y + c[0].y) + e1d, perp01.z * t03 + c[0].z);
        let t12 = horizontal(c[2].x - c[1].x, c[2].z - c[1].z) / 3.0;
        let r1p3 = Vec3::new(perp01.x * t12 + c[1].x, (t12 * perp01.y + c[1].y) + e1d, perp01.z * t12 + c[1].z);
        let perp23 = {
            let (dx, dz) = (c[3].x - c[2].x, c[3].z - c[2].z);
            let l = ((dz * dz + 0.0) + dx * dx).sqrt();
            if l == 0.0 {
                Vec3::ZERO
            } else {
                let inv = 1.0 / l;
                Vec3::new((-dz) * inv, inv * 0.0, dx * inv)
            }
        };
        let e0d = e0 / 3.0;
        let t30 = horizontal(c[0].x - c[3].x, c[0].z - c[3].z) / 3.0;
        let r2p0 = Vec3::new(perp23.x * t30 + c[3].x, (t30 * perp23.y + c[3].y) - e0d, perp23.z * t30 + c[3].z);
        let t21 = horizontal(c[1].x - c[2].x, c[1].z - c[2].z) / 3.0;
        let r2p3 = Vec3::new(perp23.x * t21 + c[2].x, (t21 * perp23.y + c[2].y) - e0d, perp23.z * t21 + c[2].z);
        let ends = [(c[0], c[1]), (r1p0, r1p3), (r2p0, r2p3), (c[3], c[2])];
        let g: Vec<[Vec3; 4]> = ends
            .iter()
            .map(|&(p0, p3)| {
                let p1 = Vec3::new((p3.x - p0.x) * THIRD + p0.x, (p3.y - p0.y) * THIRD + p0.y, (p3.z - p0.z) * THIRD + p0.z);
                let p2 = Vec3::new(p3.x + (p0.x - p3.x) * THIRD, p3.y + (p0.y - p3.y) * THIRD, p3.z + (p0.z - p3.z) * THIRD);
                [p0, p1, p2, p3]
            })
            .collect();
        let mut v = 0.0f32;
        for _ in 0..PATCH_STEPS {
            let w = 1.0 - v;
            let cols: Vec<Vec3> = (0..4).map(|k| bern3(g[0][k], g[1][k], g[2][k], g[3][k], w, v)).collect();
            let mut u = 0.0f32;
            for i in 0..PATCH_STEPS {
                let p = bern3(cols[0], cols[1], cols[2], cols[3], 1.0 - u, u);
                let y = if i == 0 || i == PATCH_STEPS - 1 { PATCH_EDGE_HEIGHT } else { p.y };
                m.verts.push(Vec3::new(p.x, y, p.z));
                u += PATCH_STEP;
            }
            v += PATCH_STEP;
        }
        for row in 0..16 {
            for col in 0..16 {
                let a = row * 17 + col;
                m.tris.push([a, a + 1, a + 18]);
                m.tris.push([a, a + 18, a + 17]);
            }
        }
        m.pieces[6] = m.tris.len() as i32;
        self.add(m)
    }

    /// dynamic_mesh_rasterize_to_grid, then the mesh is counted: its bounds, each triangle's normal and bounds, and
    /// the grid cells its bounds cover.
    fn add(&mut self, mut m: TrackMesh) -> usize {
        let (mut mn, mut mx) = ([BOX_INIT; 3], [-BOX_INIT; 3]);
        for v in &m.verts {
            let v = v.to_array();
            for k in 0..3 {
                if mn[k] > v[k] {
                    mn[k] = v[k];
                }
                if v[k] > mx[k] {
                    mx[k] = v[k];
                }
            }
        }
        m.box_min = Vec3::from_array(mn);
        m.box_max = Vec3::from_array(mx);
        let vert = |i: i32| m.verts[i as usize];
        m.normals = m.tris.iter().map(|t| calculate_face_normal(vert(t[0]), vert(t[1]), vert(t[2]))).collect();
        m.tri_boxes = m
            .tris
            .iter()
            .map(|t| {
                let (mut lo, mut hi) = (vert(t[0]).to_array(), vert(t[0]).to_array());
                for &i in &t[1..] {
                    let v = vert(i).to_array();
                    for k in 0..3 {
                        if lo[k] > v[k] {
                            lo[k] = v[k];
                        }
                        if v[k] > hi[k] {
                            hi[k] = v[k];
                        }
                    }
                }
                [Vec3::from_array(lo), Vec3::from_array(hi)]
            })
            .collect();
        let id = self.meshes.len();
        let (x0, z0) = (cvtt(mn[0] * CELL_SCALE), cvtt(mn[2] * CELL_SCALE));
        let (x1, z1) = (cvtt(mx[0] * CELL_SCALE), cvtt(mx[2] * CELL_SCALE));
        for z in z0..=z1 {
            if z as u32 > 63 {
                continue;
            }
            for x in x0..=x1 {
                if x as u32 > 63 {
                    continue;
                }
                let cell = &mut self.grid[z as usize * GRID_CELLS + x as usize];
                if cell.len() < CELL_MESHES {
                    cell.push(id as i32);
                }
            }
        }
        self.meshes.push(m);
        id
    }
}

fn minss(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

/// A triangle collected from the track, as its three corners.
pub type TrackTriangle = [Vec3; 3];

const MAX_CANDIDATES: usize = 64;
const MAX_TRIANGLES: usize = 1024;
/// Which parts track_collect_triangles_in_bounds takes: a tube's rails, its bed, and its walls (all of a ramp).
pub const RAILS: u32 = 1;
pub const BED: u32 = 2;
pub const WALLS: u32 = 4;

fn boxes_overlap(lo: Vec3, hi: Vec3, min: Vec3, max: Vec3) -> bool {
    !(lo.x > max.x) && !(lo.y > max.y) && !(lo.z > max.z) && !(min.x > hi.x) && !(min.y > hi.y) && !(min.z > hi.z)
}

impl Track {
    /// track_collect_triangles_in_bounds: the triangles of the chosen parts of every mesh listed in the grid cells
    /// the bounds cover (a mesh in several cells is taken once per cell) whose bounds meet them.
    pub fn collect(&self, min: Vec3, max: Vec3, parts: u32) -> Vec<TrackTriangle> {
        let mut out = Vec::new();
        if self.meshes.is_empty() {
            return out;
        }
        let (x0, x1) = (cvtt(min.x * CELL_SCALE), cvtt(max.x * CELL_SCALE));
        let (z0, z1) = (cvtt(min.z * CELL_SCALE), cvtt(max.z * CELL_SCALE));
        let mut candidates = Vec::new();
        for z in z0..=z1 {
            if z as u32 > 63 {
                continue;
            }
            for x in x0..=x1 {
                if x as u32 > 63 {
                    continue;
                }
                for &m in &self.grid[z as usize * GRID_CELLS + x as usize] {
                    if candidates.len() < MAX_CANDIDATES {
                        candidates.push(m);
                    }
                }
            }
        }
        for m in candidates {
            let mesh = &self.meshes[m as usize];
            if !boxes_overlap(mesh.box_min, mesh.box_max, min, max) {
                continue;
            }
            for piece in 0..3 {
                if parts & (1 << piece) != 0 {
                    mesh.collect_piece(piece, min, max, &mut out);
                }
            }
        }
        out
    }
}

impl TrackMesh {
    /// train_collect_rail_points for the rails then the bed: every triangle of both (at most 1024).
    pub fn rail_triangles(&self) -> Vec<TrackTriangle> {
        (0..2).flat_map(|p| self.pieces[p]..self.pieces[p + 4]).take(MAX_TRIANGLES).map(|i| self.tris[i as usize].map(|v| self.verts[v as usize])).collect()
    }

    /// track_collect_overlapping_triangles.
    fn collect_piece(&self, piece: usize, min: Vec3, max: Vec3, out: &mut Vec<TrackTriangle>) {
        for i in self.pieces[piece]..self.pieces[piece + 4] {
            let b = self.tri_boxes[i as usize];
            if out.len() < MAX_TRIANGLES && boxes_overlap(b[0], b[1], min, max) {
                out.push(self.tris[i as usize].map(|v| self.verts[v as usize]));
            }
        }
    }
}

/// segment_intersect_mesh: the nearest triangle the segment enters, as (fraction, point, normal).
pub fn segment_intersect_triangles(tris: &[TrackTriangle], start: Vec3, end: Vec3) -> Option<(f32, Vec3, Vec3)> {
    let mut best = (1.0f32, Vec3::ZERO, Vec3::ZERO);
    for &[a, b, c] in tris {
        let n = calculate_face_normal(a, b, c);
        if let Some((t, p)) = crate::world::collide::segment_intersect_face(n, start, end, a, b, c)
            && !(best.0 <= t)
        {
            best = (t, p, n);
        }
    }
    (1.0 > best.0).then_some(best)
}
