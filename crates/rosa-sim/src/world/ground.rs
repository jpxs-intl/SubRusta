use std::collections::HashMap;

use glam::Vec3;

use crate::world::{
    collide::{TraceHit, calculate_face_normal, segment_intersect_face},
    roads::{CityBlock, RoadNetwork},
};

const TDIM: usize = 2049;
pub(crate) const ORIGIN: f32 = -4096.0;
const NAV: i32 = 16384;
const MASK: usize = 4096;

const BLOCK_IDS: [usize; 28] = [
    2, 40, 32, 46, 45, 47, 28, 10, 33, 42, 35, 13, 59, 31,
    52, 48, 50, 19, 22, 58, 62, 61, 41, 36, 43, 44, 38, 39,
];

const NEIGH: [(i32, i32); 8] = [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)];

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

pub struct Ground {
    base: Vec<f32>,
    roadmap: HashMap<u32, f32>,
    mask: Vec<u64>,
    bounds: Option<(i32, i32, i32, i32)>,
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

    pub fn set_street_bounds(&mut self, bounds: (i32, i32, i32, i32)) {
        self.bounds = Some(bounds);
    }

    pub fn stamp_roadmap(&mut self, x: i32, z: i32, height: f32) {
        if self.roundcity || !(0..=4094).contains(&x) || !(0..=4094).contains(&z) {
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
            && x + 1024 <= 4095 && z + 1024 <= 4095
        {
            let i = (z + 1024) as usize * MASK + (x + 1024) as usize;
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

    fn build_block(&mut self, b: &CityBlock) {
        let (x0, z0) = (b.x0 * 4, b.z0 * 4);

        let w = b.x1 * 4 - x0;
        let d = b.z1 * 4 - z0;
        if w < 0 || d < 0 {
            return;
        }

        let corner = |y: i32| (y as f32) * 4.0 + 4.0 + 0.25;
        let (ha, hb, hc, hd) = (corner(b.ya), corner(b.yb), corner(b.yc), corner(b.yd));

        let wu = (w + 1) as usize;
        let at = |lx: i32, lz: i32| lz as usize * wu + lx as usize;
        let mut h = vec![0f32; wu * (d + 1) as usize];
        let mut rec = vec![Rec::default(); h.len()];

        let (inv_w, inv_d) = (1.0 / w as f32, 1.0 / d as f32);
        let mut v = 0f32;

        for lz in 0..=d {
            let om = 1.0 - v;
            let west = ha * om + hc * v;
            let east = hd * v + om * hb;
            let mut u = 0f32;
            for lx in 0..=w {
                h[at(lx, lz)] = east * u + (1.0 - u) * west;
                u += inv_w;
            }

            v += inv_d;
        }

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
            for _ in 0..16 {
                for lz in 1..d {
                    for lx in 1..w {
                        let i = at(lx, lz);
                        if rec[i].n != 0 && !(1.0 > rec[i].s[0].w) {
                            continue;
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

    pub fn block_rects(&self) -> Vec<(i32, i32, i32, i32)> {
        self.blocks.iter().map(|b| (b.nx0 - 4096, b.nz0 - 4096, b.nx0 - 4096 + b.w, b.nz0 - 4096 + b.d)).collect()
    }

    pub fn roadmap_height(&self, x: i32, z: i32) -> f32 { self.roadmap_at(x, z) }

    pub fn mask_present(&self, mx: i32, mz: i32) -> bool {
        (0..MASK as i32).contains(&mx) && (0..MASK as i32).contains(&mz) && self.covered(mx, mz)
    }

    pub fn collision_vertex(&self, nx: i32, nz: i32) -> f32 { self.vertex(nx, nz) }

    fn vertex(&self, nx: i32, nz: i32) -> f32 {
        for b in self.blocks.iter().rev() {
            let (lx, lz) = (nx - b.nx0, nz - b.nz0);
            if lx >= 0 && lz >= 0 && lx <= b.w && lz <= b.d {
                return b.h[lz as usize * (b.w + 1) as usize + lx as usize];
            }
        }
        if nx >= NAV || nz >= NAV {
            return 0.0;
        }
        let (c, u) = ((nx >> 3) as usize, (nx & 7) as f32 * 0.125);
        let (r, v) = ((nz >> 3) as usize, (nz & 7) as f32 * 0.125);
        let t = |rr: usize, cc: usize| self.base[rr * TDIM + cc];
        let (t00, t01, t10, t11) = (t(r, c), t(r, c + 1), t(r + 1, c), t(r + 1, c + 1));
        let ov = 1.0 - v;
        (1.0 - u) * (t00 * ov + t10 * v) + u * (v * t11 + ov * t01)
    }

    pub(crate) fn cell_ok(&self, ix: i32, iz: i32) -> bool {
        (0..NAV).contains(&ix) && (0..NAV).contains(&iz) && self.covered(ix >> 2, iz >> 2)
    }

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

    pub fn line_intersect_landscape(&self, start: Vec3, end: Vec3) -> Option<TraceHit> {
        let (s, e) = ([start.x, start.y, start.z], [end.x, end.y, end.z]);
        let origin = [ORIGIN, 0.0, ORIGIN];
        let mut cur = [0i32; 3];
        let mut last = [0i32; 3];
        let mut step = [0i32; 3];
        let mut ad = [0f32; 3];
        for i in 0..3 {
            cur[i] = (s[i] - origin[i]) as i32;
            last[i] = (e[i] - origin[i]) as i32;
            let d = if i == 1 { 0.0 } else { e[i] - s[i] };
            step[i] = if i != 1 && d > 0.0 { 1 } else { -1 };
            ad[i] = d.abs();
        }

        let mut f = [0f32; 3];
        for i in 0..3 {
            let mut v = (s[i] as f64 - (s[i].floor() as f64 + 0.5)) as f32;
            if step[i] > 0 {
                v = -v;
            }
            f[i] = v + v;
        }

        let mut a = ad[1] * f[0] - ad[0];
        let mut dd = ad[2] * f[0] - ad[0];
        let mut b = ad[0] * f[1] - 0.0;
        let mut ee = ad[2] * f[1] - 0.0;
        let mut c = ad[0] * f[2] - ad[2];
        let mut g = f[2] * ad[1] - ad[2];

        let limit = ((last[0] - cur[0]).abs() + (last[2] - cur[2]).abs()).min(16384);
        let mut best = TraceHit { fraction: 1.0, pos: Vec3::ZERO, normal: Vec3::ZERO };
        let (mut cx, mut cz) = (cur[0], cur[2]);
        let mut n = 0;

        loop {
            if self.cell_ok(cx, cz) {
                self.test_cell(cx, cz, start, end, &mut best);
            }

            if best.fraction < 1.0 {
                return Some(best);
            }

            if cx == last[0] && cz == last[2] {
                return None;
            }

            if a < b && dd < c {
                cx += step[0];
                b -= ad[1];
                a += ad[1];
                c -= ad[2];
                dd += ad[2];
            } else if !(g < ee) {
                b += ad[0];
                a -= ad[0];
                ee += ad[2];
                g -= ad[2];
            } else {
                cz += step[2];
                c += ad[0];
                dd -= ad[0];
                ee -= ad[1];
                g += ad[1];
            }

            n += 1;

            if n > limit {
                return None;
            }
        }
    }

    fn test_cell(&self, cx: i32, cz: i32, start: Vec3, end: Vec3, best: &mut TraceHit) {
        let p = |dx: i32, dz: i32| {
            Vec3::new((cx + dx) as f32 + ORIGIN, self.vertex(cx + dx, cz + dz), (cz + dz) as f32 + ORIGIN)
        };

        let (v00, v10, v11, v01) = (p(0, 0), p(1, 0), p(1, 1), p(0, 1));

        for [ta, tb, tc] in [[v00, v10, v11], [v00, v11, v01]] {
            let n = calculate_face_normal(ta, tb, tc);

            if let Some((t, pos)) = segment_intersect_face(n, start, end, ta, tb, tc) && best.fraction > t {
                *best = TraceHit { fraction: t, pos, normal: n };
            }
        }
    }
}

pub fn generate_grass(do_hack: bool) -> Vec<f32> {
    let mut h = vec![0f32; 2049 * 2049];

    for z in 0..2049 {

        let dz640 = (z as i32 - 640) as f32;
        let dz640_sq = dz640 * dz640;
        let dz704 = (z as i32 - 704) as f32;

        for x in 0..2049 {
            let dx640 = (x as i32 - 640) as f32;
            let dist1 = (dx640 * dx640 + dz640_sq).sqrt();
            let nrm = (dist1 * 0.0078125 - 1.5) * 0.5;
            let (a, b, c): (f32, f64, f64) = if nrm <= 0.0 {
                (0.0, 0.0, 0.0)
            } else if nrm <= 3.0 {
                let s = nrm * nrm;
                (s, 0.25 * s as f64, s as f64)
            } else {
                (9.0, 2.25, 9.0)
            };

            let n1 = value_noise_2d(u32::MAX, x as f32 * 0.03125, z as f32 * 0.03125);
            let n2 = value_noise_2d(u32::MAX, x as f32 * 0.015625, z as f32 * 0.015625);
            let n3 = value_noise_2d(u32::MAX, x as f32 * 0.0078125, z as f32 * 0.0078125);

            let acc32: f32 = 16.0 * (n2 + a) + (23.75 + 8.0 * n1);
            let acc64: f64 = (n3 as f64 + b) * 32.0 * c + acc32 as f64;
            let mut height = acc64 as f32;

            let dx704 = (x as i32 - 704) as f32;
            let dist2 = (dx704 * dx704 + dz704 * dz704).sqrt();
            let v = 0.015625 * (dist2 - 320.0);

            if v < 0.0 {
                height = 23.875;
            } else if v <= 1.0 {
                height = height * v + (1.0 - v) * 23.875;
            }

            if do_hack {
                let (xi, zi) = (x as i32, z as i32);
                let edge = |boundary: i32, coord: i32| -> f32 {
                    let raw = (boundary - coord) as f32 * 0.25;
                    if raw <= 1.0 { 1.0 - raw } else { 0.0 }
                };

                let ramp = |t: f32| if t > 1.0 { 23.75 } else { (3.0 - 2.0 * t) * t * t * 12.0 + 11.75 };

                if zi <= 637 && xi <= 767 {
                    height = ramp(edge(638, zi).max(edge(768, xi)));
                }
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

    let ix = x as i32;
    let iy = y as i32;
    let fx = x - ix as f32;
    let fy = y - iy as f32;

    let iy57 = iy.wrapping_mul(57);
    let ix1 = ix.wrapping_add(1) & mask;
    let iy1_57 = (iy.wrapping_add(1) & mask).wrapping_mul(57);

    let c00 = ix.wrapping_add(iy57);
    let c10 = iy57.wrapping_add(ix1);
    let c01 = ix.wrapping_add(iy1_57);
    let c11 = iy1_57.wrapping_add(ix1);

    let corner = |n: i32| -> f32 {
        let h = n ^ (n << 13);
        let m = h.wrapping_mul(h).wrapping_mul(15731).wrapping_add(789221);
        let k = m.wrapping_mul(m).wrapping_add(1376312589);
        1.0 - (k as f32) / 1073741824.0
    };

    let sx = (3.0 - (fx + fx)) * fx * fx;
    let sy = (3.0 - (fy + fy)) * fy * fy;

    let bottom = corner(c00) * (1.0 - sx) + corner(c10) * sx;
    let top = corner(c11) * sx + (1.0 - sx) * corner(c01);
    bottom * (1.0 - sy) + sy * top
}
