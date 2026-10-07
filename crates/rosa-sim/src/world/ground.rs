// IM GOING TO BE 100% TRANSPARENT
// Claude wrote this
// Why? Well dear reader, let me tell you:
//   1. Alex's system is weird, and its heavily optimized by the GC
//   2. Alex's system DOESNT transmit this data to the client, the client renders it from this same math
//   3. Because of #2, the client and server heights need to be 1:1 or the client sees clipping
//   4. You want to sit here and do this shit?
// -------------------------- END OF AI DISCLAIMER -------------------------------

use std::collections::HashMap;

use glam::Vec3;

use crate::world::roads::{CityBlock, RoadNetwork};

const TDIM: usize = 2049; // generate_grass grid (2048 + 1)
const ORIGIN: f32 = -4096.0; // terrain world origin (data_5d31560)
const NAV: i32 = 16384; // collision vertex grid, 1 unit per vertex: index = world + 4096
const MASK: usize = 4096; // coverage mask, 4-unit cells: index = (world + 4096) / 4

/// Blocks rewritten by build_traffic_navmap → navmap_build_intersection_distance_field.
/// Hardcoded in the binary (test2's layout), applied on every non-round map, in this order.
const BLOCK_IDS: [usize; 28] = [
    2, 0x28, 0x20, 0x2e, 0x2d, 0x2f, 0x1c, 0xa, 0x21, 0x2a, 0x23, 0xd, 0x3b, 0x1f,
    0x34, 0x30, 0x32, 0x13, 0x16, 0x3a, 0x3e, 0x3d, 0x29, 0x24, 0x2b, 0x2c, 0x26, 0x27,
];

/// Neighbour visit order of the distance-field relaxation (rows, then columns; centre skipped).
const NEIGH: [(i32, i32); 8] = [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)];

/// A city block whose collision vertices were rewritten (plane + distance-field blend).
/// Covers collision vertices [nx0, nx0+w] x [nz0, nz0+d].
struct Block {
    nx0: i32,
    nz0: i32,
    w: i32,
    d: i32,
    h: Vec<f32>,
}

#[derive(Clone, Copy, Default)]
struct Src {
    x: i32,
    z: i32,
    w: f32,
}

#[derive(Clone, Copy, Default)]
struct Rec {
    n: u8,
    s: [Src; 2],
}

/// Terrain collision surface, mirroring the dedicated server:
/// - collision vertices are 1 unit apart: the generate_grass field upsampled 8x (build_traffic_navmap),
///   except inside the hardcoded city blocks, which are rewritten from the roadmap placement map;
/// - the surface is 2 triangles per 1-unit cell (line_intersect_landscape);
/// - a 4-unit coverage mask removes terrain under stamped structures.
///   Structures themselves (roads/buildings/sectors) are separate collision in the AreaGrid.
pub struct Ground {
    base: Vec<f32>,             // generate_grass field, TDIM x TDIM, index z*TDIM + x
    roadmap: HashMap<u32, f32>, // placement heights (map_data_buf), key = (z << 12) | x, cells; absent = 0
    mask: Vec<u64>,             // MASK x MASK bits, 1 = terrain present
    bounds: Option<(i32, i32, i32, i32)>, // street-intersection bbox in cells: (minx, maxx, minz, maxz)
    roundcity: bool,
    blocks: Vec<Block>,
}

impl Ground {
    pub fn new(base: Vec<f32>, roundcity: bool) -> Self {
        assert_eq!(base.len(), TDIM * TDIM, "generate_grass must be {TDIM}x{TDIM}");
        Self {
            base,
            roadmap: HashMap::new(),
            mask: vec![u64::MAX; MASK * MASK / 64],
            bounds: None,
            roundcity,
            blocks: Vec::new(),
        }
    }

    pub fn base(&self) -> &[f32] { &self.base }
    pub fn roundcity(&self) -> bool { self.roundcity }

    /// level_data.minimum/maximumStreetIntersection — gates mask clearing.
    pub fn set_street_bounds(&mut self, bounds: (i32, i32, i32, i32)) {
        self.bounds = Some(bounds);
    }

    /// Port of roadmap_stamp_cell: max-blend `height` into the 4 corners of cell (x,z) of the
    /// placement map, and clear the terrain mask there if inside the street-intersection bbox.
    /// Called by sectors, buildings (offset.y layer) and streets — not by intersection pads.
    pub fn stamp_roadmap(&mut self, x: i32, z: i32, height: f32) {
        if self.roundcity || !(0..=0xffe).contains(&x) || !(0..=0xffe).contains(&z) {
            return;
        }
        for (dx, dz) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let key = (((z + dz) as u32) << 12) | (x + dx) as u32;
            let cur = self.roadmap.get(&key).copied().unwrap_or(0.0);
            if height > cur {
                self.roadmap.insert(key, height);
            }
        }
        if let Some((minx, maxx, minz, maxz)) = self.bounds
            && minx <= x && x <= maxx && minz <= z && z <= maxz
            && x + 0x400 <= 0xfff && z + 0x400 <= 0xfff
        {
            let i = (z + 0x400) as usize * MASK + (x + 0x400) as usize;
            self.mask[i >> 6] &= !(1u64 << (i & 63));
        }
    }

    fn roadmap_at(&self, x: i32, z: i32) -> f32 {
        if !(0..4096).contains(&x) || !(0..4096).contains(&z) {
            return 0.0;
        }
        self.roadmap.get(&(((z as u32) << 12) | x as u32)).copied().unwrap_or(0.0)
    }

    fn covered(&self, mx: i32, mz: i32) -> bool {
        let i = mz as usize * MASK + mx as usize;
        (self.mask[i >> 6] >> (i & 63)) & 1 != 0
    }

    /// Run the binary's per-block terrain rewrite. Call after ALL roadmap stamping is done
    /// (build_traffic_navmap runs at the end of load_map). Skipped on round-city.
    pub fn build_city_blocks(&mut self, roads: &RoadNetwork) {
        if self.roundcity {
            return;
        }
        for id in BLOCK_IDS {
            if let Some(b) = roads.city_block(id) {
                self.build_block(&b);
            }
        }
    }

    /// Port of navmap_build_intersection_distance_field for one block.
    fn build_block(&mut self, b: &CityBlock) {
        let (x0, z0) = (b.x0 * 4, b.z0 * 4); // rcx_2, r11_1 (world units, before +4096)
        let w = b.x1 * 4 - x0; // r15_2
        let d = b.z1 * 4 - z0; // rax_18
        if w < 0 || d < 0 {
            return;
        }
        let corner = |y: i32| (y as f32) * 4.0 + 4.0 + 0.25;
        let (ha, hb, hc, hd) = (corner(b.ya), corner(b.yb), corner(b.yc), corner(b.yd));

        let wu = (w + 1) as usize;
        let at = |lx: i32, lz: i32| lz as usize * wu + lx as usize;
        let mut h = vec![0f32; wu * (d + 1) as usize];
        let mut rec = vec![Rec::default(); h.len()];

        // 1) plane through the four intersection heights
        let (inv_w, inv_d) = (1.0 / w as f32, 1.0 / d as f32);
        let mut v = 0f32;
        for lz in 0..=d {
            let om = 1.0 - v;
            let west = ha * om + hc * v; // A (z0) .. C (z1)
            let east = hd * v + om * hb; // B (z0) .. D (z1)
            let mut u = 0f32;
            for lx in 0..=w {
                h[at(lx, lz)] = east * u + (1.0 - u) * west;
                u += inv_w;
            }
            v += inv_d;
        }

        // 2) seeds: cells whose 4 roadmap corners are all stamped take the roadmap height
        let (wc, dc) = (w >> 2, d >> 2);
        if dc > 0 && wc > 0 {
            for cz in 0..dc {
                for cx in 0..wc {
                    let (gx, gz) = (b.x0 + cx, b.z0 + cz);
                    let r00 = self.roadmap_at(gx, gz);
                    let r10 = self.roadmap_at(gx + 1, gz);
                    let r01 = self.roadmap_at(gx, gz + 1);
                    let r11 = self.roadmap_at(gx + 1, gz + 1);
                    if !(r00 > 0.0 && r10 > 0.0 && r01 > 0.0 && r11 > 0.0) {
                        continue;
                    }
                    let mut v = 0f32;
                    for j in 0..5 {
                        let om = 1.0 - v;
                        let left = r00 * om + r01 * v;
                        let right = r11 * v + om * r10;
                        let mut u = 0f32;
                        for k in 0..5 {
                            let (lx, lz) = (cx * 4 + k, cz * 4 + j);
                            let i = at(lx, lz);
                            if rec[i].n == 0 {
                                rec[i].n = 1;
                                rec[i].s[0] = Src { x: lx, z: lz, w: 1.0 };
                                h[i] = right * u + (1.0 - u) * left;
                            }
                            u += 0.25;
                        }
                        v += 0.25;
                    }
                }
            }
        }

        if d > 1 && w > 1 {
            // 3) 16 relaxation passes: each vertex keeps up to 2 nearest seeds (different directions),
            //    weight = 1 - dist/16
            for _ in 0..16 {
                for lz in 1..d {
                    for lx in 1..w {
                        let i = at(lx, lz);
                        if rec[i].n != 0 && !(1.0 > rec[i].s[0].w) {
                            continue; // seed
                        }
                        for (dz, dx) in NEIGH {
                            let nb = rec[at(lx + dx, lz + dz)];
                            for s in &nb.s[..nb.n as usize] {
                                let ddx = (s.x - lx) as f32;
                                let ddz = (s.z - lz) as f32;
                                let dist = (ddx * ddx + ddz * ddz).sqrt();
                                let nw = 1.0 - dist * 0.0625;
                                if !(nw > 0.0) {
                                    continue;
                                }
                                let (ndx, ndz) = if dist == 0.0 {
                                    (0.0, 0.0)
                                } else {
                                    let inv = 1.0 / dist;
                                    (ddx * inv, ddz * inv)
                                };
                                let src = Src { x: s.x, z: s.z, w: nw };
                                let r = &mut rec[i];
                                let n = r.n as usize;
                                if n == 0 {
                                    r.s[0] = src;
                                    r.n = 1;
                                    continue;
                                }
                                let mut mark: i32 = -1;
                                for (ei, e) in r.s[..n].iter().enumerate() {
                                    let edx = (e.x - lx) as f32;
                                    let edz = (e.z - lz) as f32;
                                    let ed = (edx * edx + edz * edz).sqrt();
                                    let (ex, ez) = if ed == 0.0 {
                                        (0.0, 0.0)
                                    } else {
                                        let inv = 1.0 / ed;
                                        (edx * inv, edz * inv)
                                    };
                                    if ez * ndz + ex * ndx > 0.0 {
                                        mark = if nw > e.w { ei as i32 } else { -2 };
                                    }
                                }
                                match mark {
                                    -2 => {}
                                    -1 => {
                                        if n != 2 {
                                            r.s[n] = src;
                                            r.n += 1;
                                        }
                                    }
                                    m => r.s[m as usize] = src,
                                }
                            }
                        }
                    }
                }
            }

            // 4) blend each vertex from the plane toward its seed height(s)
            let smooth = |t: f32| t * t * (3.0 - (t + t));
            for lz in 1..d {
                for lx in 1..w {
                    let r = rec[at(lx, lz)];
                    if r.n == 0 {
                        continue;
                    }
                    let own = h[at(lx, lz)];
                    let s0 = r.s[0];
                    let h0 = h[at(s0.x, s0.z)];
                    let (blend, wmax) = if r.n == 1 {
                        (h0, s0.w.max(0.0))
                    } else {
                        let s1 = r.s[1];
                        let h1 = h[at(s1.x, s1.z)];
                        let sum = s0.w + 0.0 + s1.w;
                        let t = if sum > 1.0 { s0.w } else { s0.w / sum };
                        let (p0, p1) = if 0.0 > t {
                            (h0 * 0.0, h1)
                        } else if t <= 1.0 {
                            let s = smooth(t);
                            (h0 * s, h1 * (1.0 - s))
                        } else {
                            (h0, h1 * 0.0)
                        };
                        (p1 + p0, s1.w.max(s0.w.max(0.0)))
                    };
                    h[at(lx, lz)] = if 0.0 > wmax {
                        own + blend * 0.0
                    } else if wmax <= 1.0 {
                        let s = smooth(wmax);
                        own * (1.0 - s) + blend * s
                    } else {
                        own * 0.0 + blend
                    };
                }
            }
        }

        self.blocks.push(Block { nx0: x0 + 4096, nz0: z0 + 4096, w, d, h });
    }

    /// World-space rectangles (x0, z0, x1, z1) of the rewritten city blocks, in build order.
    pub fn block_rects(&self) -> Vec<(i32, i32, i32, i32)> {
        self.blocks.iter().map(|b| (b.nx0 - 4096, b.nz0 - 4096, b.nx0 - 4096 + b.w, b.nz0 - 4096 + b.d)).collect()
    }

    /// Placement height of roadmap corner (x, z), in cells (0 = never stamped).
    pub fn roadmap_height(&self, x: i32, z: i32) -> f32 { self.roadmap_at(x, z) }

    /// Terrain present in mask cell (mx, mz), mx = (world + 4096) / 4.
    pub fn mask_present(&self, mx: i32, mz: i32) -> bool {
        (0..MASK as i32).contains(&mx) && (0..MASK as i32).contains(&mz) && self.covered(mx, mz)
    }

    /// Height of collision vertex (nx, nz), nx/nz = world + 4096 (the binary's tile-buffer value).
    pub fn collision_vertex(&self, nx: i32, nz: i32) -> f32 { self.vertex(nx, nz) }

    /// Height of collision vertex (nx, nz), nx/nz = world + 4096, in 0..=NAV.
    fn vertex(&self, nx: i32, nz: i32) -> f32 {
        for b in self.blocks.iter().rev() {
            let (lx, lz) = (nx - b.nx0, nz - b.nz0);
            if lx >= 0 && lz >= 0 && lx <= b.w && lz <= b.d {
                return b.h[lz as usize * (b.w + 1) as usize + lx as usize];
            }
        }
        // the far edge (index NAV, the last tiles' seam row/column) is never written: stays 0
        if nx >= NAV || nz >= NAV {
            return 0.0;
        }
        // build_traffic_navmap: 8x bilinear upsample of generate_grass
        let (c, u) = ((nx >> 3) as usize, (nx & 7) as f32 * 0.125);
        let (r, v) = ((nz >> 3) as usize, (nz & 7) as f32 * 0.125);
        let t = |rr: usize, cc: usize| self.base[rr * TDIM + cc];
        let (t00, t01, t10, t11) = (t(r, c), t(r, c + 1), t(r + 1, c), t(r + 1, c + 1));
        let ov = 1.0 - v;
        (1.0 - u) * (t00 * ov + t10 * v) + u * (v * t11 + ov * t01)
    }

    /// Cell (ix,iz) of the collision grid exists and isn't masked out.
    fn cell_ok(&self, ix: i32, iz: i32) -> bool {
        (0..NAV).contains(&ix) && (0..NAV).contains(&iz) && self.covered(ix >> 2, iz >> 2)
    }

    /// Terrain height at world (wx,wz): the 2-triangle surface of the 1-unit cell
    /// (diagonal (0,0)-(1,1)). None where there's no terrain (off-grid or masked).
    pub fn height_at(&self, wx: f32, wz: f32) -> Option<f32> {
        let (lx, lz) = (wx - ORIGIN, wz - ORIGIN);
        let (ix, iz) = (lx.floor() as i32, lz.floor() as i32);
        if !self.cell_ok(ix, iz) {
            return None;
        }
        let (tx, tz) = (lx - ix as f32, lz - iz as f32);
        let h00 = self.vertex(ix, iz);
        let h10 = self.vertex(ix + 1, iz);
        let h01 = self.vertex(ix, iz + 1);
        let h11 = self.vertex(ix + 1, iz + 1);
        Some(if tx >= tz {
            h00 + (h10 - h00) * tx + (h11 - h10) * tz
        } else {
            h00 + (h11 - h01) * tx + (h01 - h00) * tz
        })
    }

    /// Ray vs terrain (line_intersect_landscape): DDA over every crossed 1-unit cell,
    /// both triangles per cell, nearest hit.
    pub fn segment_intersect(&self, start: Vec3, end: Vec3) -> Option<Vec3> {
        let (sx, sz) = (start.x - ORIGIN, start.z - ORIGIN);
        let (ex, ez) = (end.x - ORIGIN, end.z - ORIGIN);
        let (dx, dz) = (ex - sx, ez - sz);
        let (mut cx, mut cz) = (sx.floor() as i32, sz.floor() as i32);
        let (ecx, ecz) = (ex.floor() as i32, ez.floor() as i32);
        let step_x = if dx >= 0.0 { 1 } else { -1 };
        let step_z = if dz >= 0.0 { 1 } else { -1 };
        let (mut tmax_x, tdx) = axis(sx, dx, step_x);
        let (mut tmax_z, tdz) = axis(sz, dz, step_z);

        let mut best: Option<(f32, Vec3)> = None;
        for _ in 0..=0x4000 {
            self.test_cell(cx, cz, start, end, &mut best);
            if cx == ecx && cz == ecz {
                break;
            }
            if tmax_x < tmax_z {
                cx += step_x;
                tmax_x += tdx;
            } else {
                cz += step_z;
                tmax_z += tdz;
            }
        }
        best.map(|(_, p)| p)
    }

    fn test_cell(&self, cx: i32, cz: i32, start: Vec3, end: Vec3, best: &mut Option<(f32, Vec3)>) {
        if !self.cell_ok(cx, cz) {
            return;
        }
        let p = |dx: i32, dz: i32| {
            Vec3::new(
                (cx + dx) as f32 + ORIGIN,
                self.vertex(cx + dx, cz + dz),
                (cz + dz) as f32 + ORIGIN,
            )
        };
        for tri in [[p(0, 0), p(1, 0), p(1, 1)], [p(0, 0), p(1, 1), p(0, 1)]] {
            if let Some((t, hit)) = ray_tri(start, end, tri)
                && best.is_none_or(|(bt, _)| t < bt)
            {
                *best = Some((t, hit));
            }
        }
    }
}

/// Parametric distance to the first cell boundary + per-cell increment, for the DDA.
fn axis(s: f32, d: f32, step: i32) -> (f32, f32) {
    if d == 0.0 {
        return (f32::INFINITY, f32::INFINITY);
    }
    let inv = 1.0 / d.abs();
    let frac = s - s.floor();
    let first = if step > 0 { 1.0 - frac } else { frac };
    (first * inv, inv)
}

fn ray_tri(start: Vec3, end: Vec3, tri: [Vec3; 3]) -> Option<(f32, Vec3)> {
    let dir = end - start;
    let (e1, e2) = (tri[1] - tri[0], tri[2] - tri[0]);

    let p = dir.cross(e2);
    let det = e1.dot(p);

    if det.abs() < 1e-6 {
        return None;
    }

    let inv = 1.0 / det;
    let tvec = start - tri[0];
    let u = tvec.dot(p) * inv;

    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let q = tvec.cross(e1);
    let v = dir.dot(q) * inv;

    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let t = e2.dot(q) * inv;
    if !(0.0..=1.0).contains(&t) {
        return None;
    }

    Some((t, start + dir * t))
}

pub fn generate_grass(do_hack: bool) -> Vec<f32> {
    let mut h = vec![0f32; 2049 * 2049];
    for z in 0..2049 {
        let dz640 = (z as i32 - 0x280) as f32; // z - 640
        let dz640_sq = dz640 * dz640;
        let dz704 = (z as i32 - 0x2c0) as f32; // z - 704
        for x in 0..2049 {
            // --- radial coefficient a, from center (640,640) ---
            let dx640 = (x as i32 - 0x280) as f32;
            let dist1 = (dx640 * dx640 + dz640_sq).sqrt();
            let nrm = (dist1 * 0.0078125 - 1.5) * 0.5; // (dist/128 - 1.5) * 0.5
            // binary squares in f32 then widens to f64; clamps at n=3 (→9) and n<0 (→0)
            let (a, b, c): (f32, f64, f64) = if nrm <= 0.0 {
                (0.0, 0.0, 0.0)
            } else if nrm <= 3.0 {
                let s = nrm * nrm; // f32
                (s, 0.25 * s as f64, s as f64)
            } else {
                (9.0, 2.25, 9.0)
            };

            // --- 3 octaves of value noise (each freq = half the previous) ---
            let n1 = value_noise_2d(0xffffffff, x as f32 * 0.03125, z as f32 * 0.03125);
            let n2 = value_noise_2d(0xffffffff, x as f32 * 0.015625, z as f32 * 0.015625);
            let n3 = value_noise_2d(0xffffffff, x as f32 * 0.0078125, z as f32 * 0.0078125);

            // accumulate exactly as the binary: oct1+oct2 in f32, oct3 in f64
            let acc32: f32 = 16.0 * (n2 + a) + (23.75 + 8.0 * n1);
            let acc64: f64 = (n3 as f64 + b) * 32.0 * c + acc32 as f64;
            let mut height = acc64 as f32;

            // --- falloff 2: flatten toward center (704,704) ---
            let dx704 = (x as i32 - 0x2c0) as f32;
            let dist2 = (dx704 * dx704 + dz704 * dz704).sqrt();
            let v = 0.015625 * (dist2 - 320.0);
            if v < 0.0 {
                height = 23.875; // inside r=320: flat
            } else if v <= 1.0 {
                height = height * v + (1.0 - v) * 23.875; // ramp 23.875 → terrain
            } // v > 1: leave as terrain

            if do_hack {
                // --- border ramp (binary: only when hack_roundcity_traffic == 0) ---
                let (xi, zi) = (x as i32, z as i32);
                // edge proximity: 1 - (boundary - coord)*0.25, contributing only within 4 of the boundary
                let edge = |boundary: i32, coord: i32| -> f32 {
                    let raw = (boundary - coord) as f32 * 0.25;
                    if raw <= 1.0 { 1.0 - raw } else { 0.0 }
                };
                // smoothstep 11.75 (deep interior) → 23.75 (at the edge)
                let ramp = |t: f32| if t > 1.0 { 23.75 } else { (3.0 - 2.0 * t) * t * t * 12.0 + 11.75 };

                // pass A (0x45c868): z <= 637, x <= 767 — edges z=638 and x=768
                if zi <= 637 && xi <= 767 {
                    height = ramp(edge(638, zi).max(edge(768, xi)));
                }
                // pass B (0x45c8f0): x <= 637, z <= 767. Pass A falls straight through into it, so
                // for z <= 637 it overwrites A using only the z=768 edge (0 here → 11.75).
                if zi <= 767 && xi <= 637 {
                    let t = if zi <= 637 { edge(768, zi) } else { edge(768, zi).max(edge(638, xi)) };
                    height = ramp(t);
                }
            }

            h[z * 2049 + x] = height;
        }
    }
    h
}

pub fn value_noise_2d(wrap_mask: u32, x: f32, y: f32) -> f32 {
    let mask = wrap_mask as i32;

    let ix = x as i32; // cvttss2si: truncate toward zero (not floor)
    let iy = y as i32;
    let fx = x - ix as f32; // fractional parts (can be negative, like the binary)
    let fy = y - iy as f32;

    // lattice: combined index = x + y*57 ; only the +1 neighbors get the wrap mask
    let iy57 = iy.wrapping_mul(0x39); // iy * 57
    let ix1 = ix.wrapping_add(1) & mask; // (ix+1) & mask
    let iy1_57 = (iy.wrapping_add(1) & mask).wrapping_mul(0x39); // ((iy+1)&mask) * 57

    let c00 = ix.wrapping_add(iy57); // (ix,   iy)
    let c10 = iy57.wrapping_add(ix1); // (ix+1, iy)
    let c01 = ix.wrapping_add(iy1_57); // (ix,   iy+1)
    let c11 = iy1_57.wrapping_add(ix1); // (ix+1, iy+1)

    // per-corner pseudo-random value: 1 - signed((h²·15731+789221)² + 1376312589) · 2^-30
    // (the binary squares m — imul r8d, r8d at 0x45c436 — it is NOT the textbook n*m)
    let corner = |n: i32| -> f32 {
        let h = n ^ (n << 13);
        let m = h.wrapping_mul(h).wrapping_mul(0x3d73).wrapping_add(0xc0ae5); // ·15731 +789221
        let k = m.wrapping_mul(m).wrapping_add(0x5208dd0d); // ²      +1376312589
        1.0 - (k as f32) * 9.31322575e-10 // signed →f32, ·2^-30
    };

    // smoothstep weights  s = (3 - 2t)·t²
    let sx = (3.0 - (fx + fx)) * fx * fx;
    let sy = (3.0 - (fy + fy)) * fy * fy;

    let bottom = corner(c00) * (1.0 - sx) + corner(c10) * sx; // interp X at row iy
    let top = corner(c11) * sx + (1.0 - sx) * corner(c01); // interp X at row iy+1
    bottom * (1.0 - sy) + sy * top // interp Y
}
