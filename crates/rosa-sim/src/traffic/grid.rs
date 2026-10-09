use glam::Vec3;

use super::{MAX_STREET_CARS, Traffic, maxss, minss};
use crate::world::streets::StreetMap;

/// A car covers the lanes whose start line its bounds come within this of.
const LANE_REACH: f32 = 2.0;
const NO_CAR: f32 = 65536.0;
/// lane_clearance keeps this far behind the car ahead and closes the rest of the gap over two seconds.
const FOLLOW_GAP: f32 = 2.0;
const CLOSE_RATE: f32 = 0.5;
const TICKS_PER_SECOND: f32 = 60.0;

fn inside(min: Vec3, max: Vec3, p: Vec3) -> bool {
    p.x >= min.x && max.x >= p.x && p.z >= min.z && max.z >= p.z && p.y >= min.y && max.y >= p.y
}

fn grow(min: &mut Vec3, max: &mut Vec3, p: Vec3) {
    for k in 0..3 {
        if min[k] > p[k] {
            min[k] = p[k];
        }
        if p[k] > max[k] {
            max[k] = p[k];
        }
    }
}

/// trafficgrid_addcars: each car's bounds, the street and intersection it is in, where it lies along the street, the
/// lanes it covers, and the street's list of cars.
pub fn add_cars(traffic: &mut Traffic, map: &StreetMap) {
    traffic.street_cars.iter_mut().for_each(Vec::clear);
    // TODO: skipped while the global at raw 0xa4f16e4 is set
    for id in 0..traffic.cars.len() {
        let c = &mut traffic.cars[id];
        let (s, co) = (c.yaw as f64).sin_cos();
        let (s, co) = (s as f32, co as f32);
        let p = c.pos;
        let (fx, fy, fz) = (co * c.front + p.x, 0.0 * c.front + p.y, c.front * s + p.z);
        let (bx, by, bz) = (p.x + co * c.rear, p.y + 0.0 * c.rear, p.z + c.rear * s);
        let a = Vec3::new(s + fx, fy, fz - co);
        let (mut min, mut max) = (a, a);
        grow(&mut min, &mut max, Vec3::new(fx - s, fy + 0.0, fz + co));
        grow(&mut min, &mut max, Vec3::new(bx - s, by + 0.0, bz + co));
        grow(&mut min, &mut max, Vec3::new(bx + s, by, bz - co));
        c.bounds_min = min;
        c.bounds_max = max;
        let kept = c.physical_street != -1 && map.streets.get(c.physical_street as usize).is_some_and(|st| inside(st.bounds_min, st.bounds_max, p));
        if !kept {
            c.physical_street = -1;
            for (k, st) in map.streets.iter().enumerate() {
                if inside(st.bounds_min, st.bounds_max, p) {
                    c.physical_street = k as i32;
                }
            }
        }
        c.physical_intersection = -1;
        for (k, i) in map.intersections.iter().enumerate() {
            if p.x >= i.bounds_min.x && i.bounds_max.x >= p.x && p.z >= i.bounds_min.z && i.bounds_max.z >= p.z {
                c.physical_intersection = k as i32;
            }
        }
        if c.physical_street == -1 {
            continue;
        }
        let ps = c.physical_street as usize;
        let st = &map.streets[ps];
        let d = st.dir;
        let w = map.intersections[st.intersections[0]].world_pos;
        let pmin = ((min.x - w.x) * d.x + (min.y - w.y) * d.y) + (min.z - w.z) * d.z;
        let pmax = ((max.x - w.x) * d.x + (max.y - w.y) * d.y) + (max.z - w.z) * d.z;
        let fwd = s * d.z + (co * d.x + d.y * 0.0);
        if fwd > 0.0 {
            c.front_proj = maxss(pmin, pmax);
            c.rear_proj = minss(pmin, pmax);
        } else {
            c.front_proj = minss(pmin, pmax);
            c.rear_proj = maxss(pmin, pmax);
        }
        let (ls, le) = (st.lanes[0].start, st.lanes[0].end);
        let a = (d.x * (min.x - ls.x) + (min.y - ls.y) * d.y) + (min.z - ls.z) * d.z;
        let b = (max.z - ls.z) * d.z + (d.x * (max.x - ls.x) + (max.y - ls.y) * d.y);
        c.near[0] = minss(a, b);
        c.far[0] = maxss(a, b);
        let cmin = ((min.y - le.y) * d.y + (min.x - le.x) * d.x) + (min.z - le.z) * d.z;
        let cmax = ((max.y - le.y) * d.y + d.x * (max.x - le.x)) + d.z * (max.z - le.z);
        c.far[1] = if cmax > cmin { -cmin } else { -cmax };
        c.near[1] = if cmin > cmax { -cmin } else { -cmax };
        c.lane_mask = 0;
        let side = st.side;
        for (k, l) in st.lanes.iter().enumerate() {
            let o = l.start;
            let t1 = (((min.x - o.x) * side.x + (min.y - o.y) * side.y) + (min.z - o.z) * side.z).abs();
            if LANE_REACH > t1 {
                c.lane_mask |= 1 << (k & 31);
            }
            let t2 = ((side.x * (max.x - o.x) + side.y * (max.y - o.y)) + side.z * (max.z - o.z)).abs();
            if LANE_REACH > t2 {
                c.lane_mask |= 1 << (k & 31);
            }
        }
        if traffic.street_cars[ps].len() < MAX_STREET_CARS {
            traffic.street_cars[ps].push(id);
        }
    }
}

/// traffic_compute_lane_clearance: the speed that keeps `car` behind the nearest car ahead of `pos` along `street` in
/// its own lane (when `mask` has that lane), heading the street's way (`reverse` 0) or against it.
pub fn lane_clearance(traffic: &Traffic, map: &StreetMap, car: usize, street: i32, mask: i32, reverse: bool, pos: f32) -> f32 {
    let Some(list) = usize::try_from(street).ok().and_then(|s| traffic.street_cars.get(s)) else { return NO_CAR };
    let n = super::lane_count(map, street);
    let lane = traffic.cars[car].lane;
    let mut best = NO_CAR;
    for &j in list {
        if j == car || n <= 0 {
            continue;
        }
        let o = &traffic.cars[j];
        let occupied = (0..n).contains(&lane) && (mask >> (lane & 31)) & 1 != 0 && (o.lane_mask >> (lane & 31)) & 1 != 0;
        if !occupied {
            continue;
        }
        let gap = if !reverse {
            if !(o.front_proj > pos) {
                continue;
            }
            o.rear_proj - pos
        } else {
            if !(pos > o.front_proj) {
                continue;
            }
            -(o.rear_proj - pos)
        };
        let v = ((gap - FOLLOW_GAP) * CLOSE_RATE) / TICKS_PER_SECOND + o.speed();
        best = minss(maxss(0.0, v), best);
    }
    best
}
