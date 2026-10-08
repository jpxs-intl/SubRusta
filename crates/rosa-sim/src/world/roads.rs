use glam::IVec3;

use crate::world::{
    area::{ALL_FACES, AreaGrid, BlockDims, CUBE, MESH},
    blocks::{CURB, slope_id},
    ground::Ground,
};

#[derive(Clone, Copy, PartialEq, Default)]
enum Axis {
    #[default]
    X,
    Z,
}

#[derive(Clone, Copy, PartialEq, Default)]
struct Intersection {
    pos: IVec3,
    ext_px: i32,
    ext_pz: i32,
    ext_nx: i32,
    ext_nz: i32,
    streets: [Option<usize>; 4],
}

#[derive(Clone, Copy, PartialEq, Default)]
struct Street {
    i0: usize,
    i1: usize,
    axis: Axis,
    left: i32,
    right: i32,
}

pub struct CityBlock {
    pub x0: i32,
    pub x1: i32,
    pub z0: i32,
    pub z1: i32,
    pub ya: i32,
    pub yb: i32,
    pub yc: i32,
    pub yd: i32,
}

pub struct RoadNetwork {
    intersections: Vec<Intersection>,
    streets: Vec<Street>,
}

impl RoadNetwork {
    pub fn from_city(city: &rosa_map::file_types::sbc::CityFileSBC) -> Self {
        let inters: Vec<IVec3> = city.intersections.iter().map(|i| i.0.as_ivec3()).collect();
        let streets: Vec<(usize, usize, i32, i32)> = city.streets.iter()
            .map(|s| (s.intersection_indices[0] as usize, s.intersection_indices[1] as usize, s.left_lane as i32, s.right_lane as i32))
            .collect();
        Self::build(&inters, &streets)
    }

    pub fn intersection_fields(&self) -> Vec<(IVec3, [i32; 4], [i32; 4])> {
        self.intersections.iter().map(|it| (
            it.pos,
            [it.ext_px, it.ext_pz, it.ext_nx, it.ext_nz],
            it.streets.map(|s| s.map_or(-1, |s| s as i32)),
        )).collect()
    }

    pub fn build(city_inters: &[IVec3], city_streets: &[(usize, usize, i32, i32)]) -> Self {
        let mut intersections: Vec<Intersection> = city_inters
            .iter()
            .map(|&p| Intersection { pos: IVec3::new(p.x, p.y - 1, p.z), ..Default::default() })
            .collect();

        let mut streets = Vec::with_capacity(city_streets.len());
        for (si, &(i0, i1, left, right)) in city_streets.iter().enumerate() {
            let d = intersections[i1].pos - intersections[i0].pos;
            let axis = if d.x <= d.z { Axis::Z } else { Axis::X };

            for (ii, slot) in [(i0, 0usize), (i1, 2usize)] {
                let it = &mut intersections[ii];
                match axis {
                    Axis::Z => {
                        it.streets[slot + 1] = Some(si);
                        it.ext_px = it.ext_px.max(left);
                        it.ext_nx = it.ext_nx.max(right);
                    }
                    Axis::X => {
                        it.streets[slot] = Some(si);
                        it.ext_pz = it.ext_pz.max(right);
                        it.ext_nz = it.ext_nz.max(left);
                    }
                }
            }
            streets.push(Street { i0, i1, axis, left, right });
        }
        Self { intersections, streets }
    }

    pub fn compute_world_bounds(&mut self) {
        for it in &mut self.intersections {
            for lane in [&mut it.ext_px, &mut it.ext_pz, &mut it.ext_nx, &mut it.ext_nz] {
                if *lane > 0 {
                    *lane += 1;
                }
            }
        }
    }

    pub fn bounds(&self) -> Option<(i32, i32, i32, i32)> {
        let first = self.intersections.first()?;
        Some(self.intersections.iter().fold(
            (first.pos.x, first.pos.x, first.pos.z, first.pos.z),
            |(a, b, c, d), it| (a.min(it.pos.x), b.max(it.pos.x), c.min(it.pos.z), d.max(it.pos.z)),
        ))
    }

    pub fn city_block(&self, id: usize) -> Option<CityBlock> {
        let it = &self.intersections;
        let st = &self.streets;
        let a = it.get(id)?;

        let mut b = st[a.streets[0]?].i1;
        if it[b].streets[1].is_none() {
            b = st[it[b].streets[0]?].i1;
            it[b].streets[1]?;
        }

        let mut c = st[a.streets[1]?].i1;
        let cx = match it[c].streets[0] {
            Some(s) => s,
            None => {
                c = st[it[c].streets[1]?].i1;
                it[c].streets[0]?
            }
        };

        let mut d = st[cx].i1;
        if it[d].streets[3].is_none() {
            d = st[it[d].streets[0]?].i1;
            it[d].streets[3]?;
        }

        let (b, c, d) = (&it[b], &it[c], &it[d]);
        Some(CityBlock {
            x0: a.pos.x + a.ext_px,
            x1: b.pos.x - b.ext_px,
            z0: a.pos.z + a.ext_pz,
            z1: c.pos.z - c.ext_pz,
            ya: a.pos.y,
            yb: b.pos.y,
            yc: c.pos.y,
            yd: d.pos.y,
        })
    }

    pub fn build_blocks(&self, ground: &mut Ground, area: &mut AreaGrid, dims: &dyn BlockDims) {
        for s in &self.streets {
            self.street_build_blocks(ground, area, dims, s);
        }
        for (id, it) in self.intersections.iter().enumerate() {
            intersection_build_blocks(area, dims, id as u32, it);
        }
    }

    fn street_build_blocks(&self, ground: &mut Ground, area: &mut AreaGrid, dims: &dyn BlockDims, s: &Street) {
        let i0 = self.intersections[s.i0];
        let i1 = self.intersections[s.i1];
        let x_road = s.axis == Axis::X;
        let (a0, run, c) = if x_road {
            let a0 = i0.pos.x + i0.ext_px;
            (a0, i1.pos.x - i1.ext_nx - a0, i0.pos.z)
        } else {
            let a0 = i0.pos.z + i0.ext_pz;
            (a0, i1.pos.z - i1.ext_nz - a0, i0.pos.x)
        };
        let (row_up, row_down) = if x_road { (0, 2) } else { (1, 3) };
        let curb = CURB | MESH;
        let at = move |a: i32, y: i32, cc: i32| if x_road { (a, y, cc) } else { (cc, y, a) };
        let put = |area: &mut AreaGrid, a: i32, y: i32, cc: i32, v: u32| {
            let (x, y, z) = at(a, y, cc);
            area.create_block(x, y, z, v, ALL_FACES, dims);
        };

        let (left, right) = (s.left, s.right);
        let (cl, cr) = (c - left - 1, c + right);
        let curbs_flat = |area: &mut AreaGrid, a: i32, y: i32| {
            if x_road {
                if a < 0 || y + 1 < 0 {
                    return;
                }
                if cl >= 0 {
                    put(area, a, y + 1, cl, curb);
                }
                if cr >= 0 {
                    put(area, a, y + 1, cr, curb);
                }
            } else {
                if cl >= 0 && a >= 0 && y + 1 >= 0 {
                    put(area, a, y + 1, cl, curb);
                }
                if a >= 0 && cr >= 0 && y + 1 >= 0 {
                    put(area, a, y + 1, cr, curb);
                }
            }
        };

        let mut ramp = Ramp::new(i0.pos.y, run, i1.pos.y - i0.pos.y, ground.roundcity());
        for k in 0..run.max(0) {
            let a = a0 + k;
            let y = ramp.enter(k);
            let in_window = ramp.in_window;
            let rising = ramp.rise > 0 && in_window;
            let slope = |boxes: bool| {
                let (row, col) = if rising { (row_up, ramp.counter) } else { (row_down, ramp.step - 1 - ramp.counter) };
                slope_id(ramp.step, boxes, row, col).unwrap_or(0) | MESH
            };

            if k == 0 {
                curbs_flat(area, a0, y);
            }
            if run - 1 > k {
                if in_window {
                    let id = slope(true);
                    if x_road {
                        if a >= 0 && y >= 0 {
                            if cl >= 0 {
                                put(area, a, y, cl, id);
                            }
                            if cr >= 0 {
                                put(area, a, y, cr, id);
                            }
                        }
                    } else {
                        if cl >= 0 && a >= 0 && y >= 0 {
                            put(area, a, y, cl, id);
                        }
                        if a >= 0 && cr >= 0 && y >= 0 {
                            put(area, a, y, cr, id);
                        }
                    }
                } else {
                    curbs_flat(area, a, y);
                }
            } else if run - 1 == k {
                curbs_flat(area, a, y);
            }

            for cc in (c - left)..(c + right) {
                if cc < 0 || y < 0 || a < 0 {
                    continue;
                }
                put(area, a, y, cc, if in_window { slope(false) } else { CUBE });
            }
            for i in (-left - 1)..=right {
                let (x, _, z) = at(a, y, c + i);
                ground.stamp_roadmap(x, z, y as f32 * 4.0);
            }
            ramp.leave();
        }

        for cc in (c - left)..(c + right) {
            if x_road {
                if i0.ext_px > 0 && (i0.ext_pz > 0 || i0.ext_nz > 0) {
                    let x = i0.pos.x + i0.ext_px;
                    if i0.pos.y >= 0 && cc >= 0 && x >= 0 {
                        area.create_block(x, i0.pos.y, cc, CUBE, ALL_FACES, dims);
                    }
                }
                if i1.ext_nx > 0 && (i1.ext_pz > 0 || i1.ext_nz > 0) {
                    let x = i1.pos.x - i1.ext_nx - 1;
                    if x >= 0 && i1.pos.y >= 0 && cc >= 0 {
                        area.create_block(x, i1.pos.y, cc, CUBE, ALL_FACES, dims);
                    }
                }
            } else {
                if i0.ext_pz > 0 && (i0.ext_px > 0 || i0.ext_nx > 0) {
                    let z = i0.pos.z + i0.ext_pz;
                    if cc >= 0 && i0.pos.y >= 0 && z >= 0 {
                        area.create_block(cc, i0.pos.y, z, CUBE, ALL_FACES, dims);
                    }
                }
                if i1.ext_nz > 0 && (i1.ext_px > 0 || i1.ext_nx > 0) {
                    let z = i1.pos.z - i1.ext_nz - 1;
                    if z >= 0 && i1.pos.y >= 0 && cc >= 0 {
                        area.create_block(cc, i1.pos.y, z, CUBE, ALL_FACES, dims);
                    }
                }
            }
        }
    }
}

fn intersection_build_blocks(area: &mut AreaGrid, dims: &dyn BlockDims, id: u32, it: &Intersection) {
    let (x, y, z) = (it.pos.x, it.pos.y, it.pos.z);
    let (l0, l1, l2, l3) = (it.ext_px, it.ext_pz, it.ext_nx, it.ext_nz);
    for zz in (z - l3)..(z + l1) {
        for xx in (x - l2)..(x + l0) {
            if y >= 0 && zz >= 0 && xx >= 0 {
                area.create_block(xx, y, zz, CUBE, ALL_FACES, dims);
            }
        }
    }
    let curb = CURB | MESH;
    let mut put = |xx: i32, yy: i32, zz: i32| {
        if xx >= 0 && yy >= 0 && zz >= 0 {
            area.create_block(xx, yy, zz, curb, ALL_FACES, dims);
        }
    };
    if it.streets[0].is_none() {
        for zz in (z - l3 - 1)..=(z + l1) {
            put(x + l0, y + 1, zz);
        }
    }
    if it.streets[2].is_none() {
        for zz in (z - l3 - 1)..=(z + l1) {
            put(x - l0 - 1, y + 1, zz);
        }
    }
    if it.streets[1].is_none() {
        for xx in (x - l2 - 1)..=(x + l0) {
            put(xx, y + 1, z + l1);
        }
    }
    if it.streets[3].is_none() {
        for xx in (x - l2 - 1)..=(x + l0) {
            put(xx, y + 1, z - l3 - 1);
        }
    }
    if it.streets.iter().filter(|s| s.is_some()).count() > 2 {
        let lights = [
            (2usize, x + l0, z + l1, 1u32),
            (0, x - l0 - 1, z - l1 - 1, (1 << 11) | 1),
            (3, x - l0 - 1, z + l1, (1 << 24) | (1 << 10) | 1),
            (1, x + l0, z - l1 - 1, (1 << 24) | (3 << 10) | 1),
        ];
        for (side, lx, lz, v) in lights {
            if it.streets[side].is_some() && lx >= 0 && y + 1 >= 0 && lz >= 0 {
                area.set_object(lx, y + 1, lz, id << 12 | v);
            }
        }
    }
}

struct Ramp {
    lo: i32,
    hi: i32,
    step: i32,
    rise: i32,
    y: i32,
    counter: i32,
    in_window: bool,
}

impl Ramp {
    fn new(y0: i32, run: i32, rise: i32, roundcity: bool) -> Self {
        let arise = rise.abs();
        let half = run >> 1;
        let (step, half_width) = if rise == 0 {
            (6, 0)
        } else if roundcity {
            let gentle = (arise <= run / 10) as i32;
            (gentle * 4 + 6, arise * (gentle * 2 + 3))
        } else {
            let step = (run / arise).max(1);
            (step, (arise * step) >> 1)
        };
        Self { lo: half - half_width, hi: half + half_width, step, rise, y: y0, counter: 0, in_window: false }
    }

    fn enter(&mut self, k: i32) -> i32 {
        self.in_window = self.lo <= k && self.hi > k;
        if self.in_window && self.rise > 0 && self.counter % self.step == 0 {
            self.y += 1;
        }
        self.y
    }

    fn leave(&mut self) {
        if self.in_window {
            if self.rise < 0 && self.counter % self.step == self.step - 1 {
                self.y -= 1;
            }
            self.counter = (self.counter + 1) % self.step;
        }
    }
}
