use glam::IVec3;

use crate::world::{grid::AreaGrid, ground::Ground};

const ROAD: u32 = 0x4000_0000;

#[derive(Clone, Copy, PartialEq, Default)]
enum Axis {
    #[default]
    X,
    Z,
}

/// Mirrors level_data.intersections[]. Lane extents map to the binary's lanes[0..4]:
/// ext_px = lanes[0] (+X), ext_pz = lanes[1] (+Z), ext_nx = lanes[2] (-X), ext_nz = lanes[3] (-Z).
#[derive(Clone, Copy, PartialEq, Default)]
struct Intersection {
    pos: IVec3, // pos.y stored as city_y - 1 (create_intersection_internal)
    ext_px: i32,
    ext_pz: i32,
    ext_nx: i32,
    ext_nz: i32,
    streets: [Option<usize>; 4], // streetIndices: 0 = +X, 1 = +Z, 2 = -X, 3 = -Z
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
            // create_road: signed compare, (i1 - i0).x <= (i1 - i0).z → Z road
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

    pub fn stamp(&self, ground: &mut Ground, grid: &mut AreaGrid) {
        for s in &self.streets {
            self.stamp_street(ground, grid, s);
        }
        for it in &self.intersections {
            stamp_intersection(grid, it);
        }
    }

    fn stamp_street(&self, ground: &mut Ground, grid: &mut AreaGrid, s: &Street) {
        let a = self.intersections[s.i0];
        let b = self.intersections[s.i1];
        let rc = ground.roundcity();
        match s.axis {
            Axis::X => {
                let x0 = a.pos.x + a.ext_px;
                let x1 = b.pos.x - b.ext_nx;
                let (zc, run) = (a.pos.z, x1 - x0);
                let mut ramp = Ramp::new(a.pos.y, run, b.pos.y - a.pos.y, rc);
                for k in 0..run {
                    let x = x0 + k;
                    let y = ramp.enter(k);
                    for z in (zc - s.left - 1)..=(zc + s.right) {
                        put(grid, x, y, z, ROAD);
                        ground.stamp_roadmap(x, z, y as f32 * 4.0);
                    }
                    ramp.leave();
                }
            }
            Axis::Z => {
                let z0 = a.pos.z + a.ext_pz;
                let z1 = b.pos.z - b.ext_nz;
                let (xc, run) = (a.pos.x, z1 - z0);
                let mut ramp = Ramp::new(a.pos.y, run, b.pos.y - a.pos.y, rc);
                for k in 0..run {
                    let z = z0 + k;
                    let y = ramp.enter(k);
                    for x in (xc - s.left - 1)..=(xc + s.right) {
                        put(grid, x, y, z, ROAD);
                        ground.stamp_roadmap(x, z, y as f32 * 4.0);
                    }
                    ramp.leave();
                }
            }
        }
    }
}

fn stamp_intersection(grid: &mut AreaGrid, it: &Intersection) {
    let y = it.pos.y;
    for z in (it.pos.z - it.ext_nz)..(it.pos.z + it.ext_pz) {
        for x in (it.pos.x - it.ext_nx)..(it.pos.x + it.ext_px) {
            put(grid, x, y, z, ROAD);
        }
    }

    // TODO:
    // This is missing a lot, like the edges on connected sides, and corner pieces, etc...
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

#[inline]
fn put(grid: &mut AreaGrid, x: i32, y: i32, z: i32, v: u32) {
    if x < 0 || y < 0 || z < 0 {
        return;
    }
    grid.set_cell(x as u32, y as u32, z as u32, v);
}
