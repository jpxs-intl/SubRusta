use glam::IVec3;

use crate::world::grid::AreaGrid;

const ROAD: u32 = 0x4000_0000;

#[derive(Clone, Copy, PartialEq, Default)]
enum Axis {
    #[default]
    X,
    Z
}

#[derive(Clone, Copy, PartialEq, Default)]
struct Intersection {
    pos: IVec3,
    ext_nx: i32,
    ext_px: i32,
    ext_nz: i32,
    ext_pz: i32
}

#[derive(Clone, Copy, PartialEq, Default)]
struct Street {
    i0: usize,
    i1: usize,
    axis: Axis,
    left: i32,
    right: i32
}

pub struct RoadNetwork {
    intersections: Vec<Intersection>,
    streets: Vec<Street>
}

impl RoadNetwork {
    pub fn build(city_inters: &[IVec3], city_streets: &[(usize, usize, i32, i32)]) -> Self {
        let mut intersections: Vec<Intersection> = city_inters.iter().map(|&p| Intersection {
            pos: IVec3::new(p.x, p.y - 1, p.z), ..Default::default() }).collect();

        let mut streets = Vec::with_capacity(city_streets.len());
        for &(i0, i1, left, right) in city_streets {
            let d = intersections[i1].pos - intersections[i0].pos;
            let axis = if d.x.abs() <= d.z.abs() { Axis::Z } else { Axis::X };

            for &ii in &[i0, i1] {
                let it = &mut intersections[ii];
                match axis {
                    Axis::X => { it.ext_nz = it.ext_nz.max(left); it.ext_pz = it.ext_px.max(right); }
                    Axis::Z => { it.ext_nx = it.ext_nx.max(left); it.ext_pz = it.ext_px.max(right); }
                }
            }
            streets.push(Street { i0, i1, axis, left, right });
        }
        Self { intersections, streets }
    }

    pub fn stamp(&self, grid: &mut AreaGrid) {
        for s in &self.streets { self.stamp_street(grid, s); }
        for it in &self.intersections { stamp_intersection(grid, it); }
    }

    fn stamp_street(&self, grid: &mut AreaGrid, s: &Street) {
        let a = self.intersections[s.i0];
        let b = self.intersections[s.i1];
        match s.axis {
            Axis::X => {
                let x0 = a.pos.x + a.ext_px;
                let x1 = b.pos.x + b.ext_nx;
                let (zc, run) = (a.pos.z, x1 - x0);
                for k in 0..run {
                    let x = x0 + k;
                    let y = a.pos.y + ramp(a.pos.y, b.pos.y, k, run);
                    for z in (zc - s.left - 1)..=(zc + s.right) {
                        put(grid, x, y, z, ROAD);
                    }
                }
            }
            Axis::Z => {
                let z0 = a.pos.z + a.ext_pz;
                let z1 = b.pos.z - b.ext_nz;
                let (xc, run) = (a.pos.x, z1 - z0);
                for k in 0..run {
                    let z = z0 + k;
                    let y = a.pos.y + ramp(a.pos.y, b.pos.y, k, run);
                    for x in (xc - s.left - 1)..=(xc + s.right) {
                        put(grid, x, y, z, ROAD);
                    }
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

#[inline]
fn ramp(y0: i32, y1: i32, k: i32, run: i32) -> i32 {
    if run <= 0 { 0 } else { (y1 - y0) * k / run }
}

#[inline]
fn put(grid: &mut AreaGrid, x: i32, y: i32, z: i32, v: u32) {
    if x < 0 || y < 0 || z < 0 { return; }
    grid.set_cell(x as u32, y as u32, z as u32, v);
}