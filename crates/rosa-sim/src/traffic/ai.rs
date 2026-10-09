use glam::Vec3;

use super::{
    LOOP_STREET, LOOP_STREET_2, PI, TWO_PI, Traffic, grid::lane_clearance, lane_count, left_lanes, maxss, minss, rand_bit, rand_mod, random_street,
    route::plan_route,
};
use crate::world::streets::{Lane, MapStreet, StreetMap};

/// Ticks a car may sit still before it backs out (until 319, when the count starts over).
const STUCK_TICKS: i32 = 179;
const STUCK_RESET: i32 = 319;
const STOPPED: f32 = 1.0 / 60.0;
const PUSHING: f32 = f32::from_bits(0x3d88_8889);
/// The lane change eases in at an eighth of the car's speed, at most this much a tick.
const LANE_CHANGE_RATE: f32 = 0.125;
const LANE_CHANGE_MAX: f32 = f32::from_bits(0x3c08_8889);
/// The car steers for a point up to 10 ahead, less the further it is off its lane.
const LOOK_AHEAD: f32 = 10.0;
const LOOK_FALLOFF: f32 = 0.25;
const STREET_STEER: f64 = 0.7853981633975;
const STREET_STEER_MAX: f32 = f32::from_bits(0x3f49_0fdb);
const TURN_STEER: f64 = 0.981747704246875;
const TURN_STEER_MAX: f32 = f32::from_bits(0x3f7b_53d1);
const STEER_RATE: f32 = 0.0625;
const TURN_STEER_RATE: f32 = 0.25;
const CRUISE: f32 = f32::from_bits(0x3e88_8889);
const SLOW: f32 = f32::from_bits(0x3e08_8889);
const CRAWL: f32 = 0.3;
const MIN_SPEED: f32 = f32::from_bits(0x3e08_8889);
const AGGRESSIVE_MIN: f32 = f32::from_bits(0x3e4c_cccd);
const TICKS_PER_SECOND: f32 = 60.0;
/// The point ahead of the car the remaining distance is measured from, and where the intersection begins.
const NOSE: f32 = 4.0;
const AT_JUNCTION: f32 = 1.0;
const STOP_LINE: f32 = 4.0;
const BRAKING: f32 = 6.0;
const FAST_DISTANCE: f32 = 40.0;
const QUEUE_DISTANCE: f32 = 32.0;
const QUEUE_REACH: f32 = 12.0;
const SCAN_AHEAD: f32 = 12.0;
const AMBER: i32 = 1;
const LEFT_LIGHTS: i32 = 4;
const BEZIER_EPSILON: f32 = 1.52587890625e-05;
const CORNER_BIAS: f32 = 0.125;
const NEXT_LOOK: f32 = 4.0;
const SHARP: f32 = 0.5;

fn lane_at(st: &MapStreet, i: i32) -> Lane {
    usize::try_from(i).ok().and_then(|i| st.lanes.get(i)).copied().unwrap_or_default()
}

fn neg(v: Vec3) -> Vec3 {
    Vec3::new(-v.x, -v.y, -v.z)
}

fn sincos(yaw: f32) -> (f32, f32) {
    let (s, c) = (yaw as f64).sin_cos();
    (s as f32, c as f32)
}

/// The steering angle that turns from `yaw` towards `target`, wrapped the short way and clamped.
fn steer_toward(yaw: f32, target: f32, limit: f64, max: f32) -> f32 {
    let mut a = target;
    if ((yaw - a) as f64) >= PI {
        a = (a as f64 + TWO_PI) as f32;
    }
    let mut diff = a - yaw;
    if (diff as f64) >= PI {
        diff = a - (yaw as f64 + TWO_PI) as f32;
    }
    let d = diff as f64;
    if -limit > d {
        -max
    } else if d > limit {
        max
    } else {
        diff
    }
}

/// The co-op speed for aggressive cars: (wave * `per` + 16) / 60.
fn coop_speed(traffic: &Traffic, per: i32) -> f32 {
    (traffic.coop_level * per + 16) as f32 / TICKS_PER_SECOND
}

/// The stuck count: it climbs while the car is stopped but means to go forwards, and runs on to 319 once past 179.
fn update_stuck(traffic: &mut Traffic, id: usize) {
    let c = &mut traffic.cars[id];
    if c.stuck > STUCK_TICKS {
        c.stuck += 1;
        if c.stuck > STUCK_RESET {
            c.stuck = 0;
        }
        return;
    }
    let v = c.vel;
    let speed = (v.z * v.z + (v.y * v.y + v.x * v.x)).sqrt();
    let r2 = c.rot[2];
    if STOPPED > speed && (((v.y * r2.y) + (r2.x * v.x)) + (r2.z * v.z)) + c.target_speed > PUSHING {
        c.stuck += 1;
    } else {
        c.stuck = 0;
    }
}

/// Replans from `street` once the car reaches the end of its route; an aggressive car heads for a random street.
fn replan(traffic: &mut Traffic, map: &StreetMap, id: usize, street: i32, mut dest: i32) {
    let n = map.streets.len() as i32;
    if traffic.cars[id].is_aggressive != 0 {
        dest = 0;
        if n != 0 {
            dest = random_street(map);
        }
    }
    let r = rand_bit();
    let c = &mut traffic.cars[id];
    let end = c.step(c.route_index).end;
    plan_route(c, map, street, end, dest, r);
}

/// The Round City loop: a car on one loop street heading the wrong way is turned onto the other end and replanned.
fn roundcity_flip(traffic: &mut Traffic, map: &StreetMap, id: usize, street: i32) {
    let (other, end) = if street == LOOP_STREET { (LOOP_STREET_2, 0) } else { (LOOP_STREET, 1) };
    let n = map.streets.len() as i32;
    let c = &mut traffic.cars[id];
    c.lane = lane_count(map, street) - c.lane - 1;
    let ri = c.route_index as usize;
    if let Some(step) = c.route.get_mut(ri) {
        step.end = end;
    }
    let mut dest = other;
    if rand_bit() == 0 {
        dest = rand_mod(n);
    }
    if traffic.cars[id].is_aggressive != 0 {
        dest = n;
        if n != 0 {
            dest = random_street(map);
        }
    }
    let r = rand_bit();
    let c = &mut traffic.cars[id];
    let e = c.step(c.route_index).end;
    plan_route(c, map, street, e, dest, r);
}

/// The lane a car moves to on its street, kept to the lanes heading its way (not the outermost).
fn clamp_lane(map: &StreetMap, street: i32, reverse: bool, mut nl: i32) -> i32 {
    let left = left_lanes(map, street);
    if reverse {
        if nl <= 0 {
            nl = 1;
        }
        if !(left > nl) {
            nl = left - 1;
        }
    } else {
        if left > nl {
            nl = left;
        }
        let n = lane_count(map, street);
        if n - 1 <= nl {
            nl = n - 2;
        }
    }
    nl
}

/// Now and then a car picks a lane for its next turn, or (rarely) wanders a lane over.
fn pick_lane(traffic: &mut Traffic, map: &StreetMap, id: usize, street: i32) -> i32 {
    let c = &mut traffic.cars[id];
    let mut nl = c.next_lane;
    if crate::rng::rand() & 0x7f != 0 {
        return nl;
    }
    let turn = c.step(c.route_index + 1).turn;
    if turn == 0 && crate::rng::rand() & 0xff == 0 {
        nl += if rand_bit() != 0 { 1 } else { -1 };
    }
    let reverse = c.step(c.route_index).end != 0;
    nl = match (turn, reverse) {
        (-1, true) | (1, false) => nl + 1,
        (-1, false) | (1, true) => nl - 1,
        _ => nl,
    };
    nl = clamp_lane(map, street, reverse, nl);
    c.next_lane = nl;
    nl
}

/// ai_traffic_car: lane keeping along the street, the lane for the next turn, slowing for lights and the cars ahead,
/// and the curve across an intersection.
pub fn ai_traffic_car(traffic: &mut Traffic, map: &StreetMap, id: usize) {
    update_stuck(traffic, id);
    let street = traffic.cars[id].street;
    if street == -1 {
        return cross_intersection(traffic, map, id);
    }
    let n_streets = map.streets.len() as i32;
    if traffic.roundcity && (street == LOOP_STREET || street == LOOP_STREET_2) {
        let c = &traffic.cars[id];
        let end = c.step(c.route_index).end;
        let wrong_way = if street == LOOP_STREET { end != 0 } else { end == 0 };
        if wrong_way {
            roundcity_flip(traffic, map, id, street);
        }
    }
    {
        let c = &mut traffic.cars[id];
        if c.route_len - 1 <= c.route_index {
            c.street = street;
            c.intersection = -1;
            let r = crate::rng::rand() as i32;
            let dest = if traffic.roundcity {
                if street == LOOP_STREET { LOOP_STREET_2 } else { LOOP_STREET }
            } else {
                r.wrapping_rem(n_streets)
            };
            replan(traffic, map, id, street, dest);
        }
    }
    let st = &map.streets[street as usize];
    let c = &traffic.cars[id];
    let lane = c.lane;
    let ri = c.route_index;
    let reverse = c.step(ri).end != 0;
    let l = lane_at(st, lane);
    let (mut from, to) = if reverse { (l.end, l.start) } else { (l.start, l.end) };
    let next_turn = c.step(ri + 1).turn;
    let nl = if c.is_aggressive != 0 || lane != c.next_lane { c.next_lane } else { pick_lane(traffic, map, id, street) };
    if lane != nl {
        let c = &mut traffic.cars[id];
        let p = c.lane_change;
        let s = (p * p) * (3.0 - (p + p));
        let v = c.vel;
        let speed = (v.z * v.z + (v.x * v.x + v.y * v.y)).sqrt();
        let np = minss(speed * LANE_CHANGE_RATE, LANE_CHANGE_MAX) + p;
        c.lane_change = np;
        if np > 1.0 {
            c.lane_change = 0.0;
            c.lane = c.next_lane;
        }
        let t = lane_at(st, c.next_lane).start;
        from = Vec3::new(from.x + (t.x - from.x) * s, from.y + (t.y - from.y) * s, from.z + (t.z - from.z) * s);
    }
    let c = &mut traffic.cars[id];
    let d = if c.step(c.route_index).end != 0 { neg(st.dir) } else { st.dir };
    let pos = c.pos;
    let side = st.side;
    let off = ((from.x - pos.x) * side.x + (from.y - pos.y) * side.y + (from.z - pos.z) * side.z).abs();
    let t = maxss(0.0, LOOK_AHEAD - off * LOOK_FALLOFF);
    let a = Vec3::new(d.x * t + pos.x, pos.y + d.y * t, t * d.z + pos.z);
    let proj = ((a.x - from.x) * d.x + (a.y - from.y) * d.y) + (a.z - from.z) * d.z;
    let aim_z = proj * d.z + from.z - pos.z;
    let aim_x = from.x + d.x * proj - pos.x;
    let target_yaw = (aim_z as f64).atan2(aim_x as f64) as f32;
    let want = steer_toward(c.yaw, target_yaw, STREET_STEER, STREET_STEER_MAX);
    let mut ds = want - c.steer;
    if -STEER_RATE > ds {
        ds = -STEER_RATE;
    } else if ds > STEER_RATE {
        ds = STEER_RATE;
    }
    c.steer = ds + c.steer;
    let next_street = c.step(c.route_index + 1).street;
    c.target_lane = c.lane;
    let r = rand_bit();
    let nxt = c.step(c.route_index + 1);
    if nxt.turn == 1 {
        c.target_lane = if nxt.end != 0 { r + 1 } else { !r + lane_count(map, next_street) - 1 };
    } else if nxt.turn == -1 {
        let left = left_lanes(map, next_street);
        c.target_lane = if nxt.end != 0 { left - 1 - r } else { r + left };
    }
    let (s, co) = sincos(c.yaw);
    let remaining = (d.x * (to.x - (co * NOSE + pos.x)) + d.y * 0.0) + d.z * (to.z - (s * NOSE + pos.z));
    let cur = c.step(c.route_index);
    let junction = st.intersections[(cur.end ^ 1) as usize];
    let ways = map.intersections[junction].streets;
    let mut way = if ways[2] == cur.street { 2 } else { (ways[1] == cur.street) as i32 };
    if ways[3] == cur.street {
        way = 3;
    }
    if AT_JUNCTION > remaining {
        c.intersection = junction as i32;
        c.street = -1;
    }
    let aggressive = c.is_aggressive != 0;
    let fast = aggressive && (remaining > FAST_DISTANCE || nxt.turn == 0);
    let base = if fast { coop_speed(traffic, 8) } else { CRUISE };
    let c = &mut traffic.cars[id];
    let mut approach = maxss(0.0, ((remaining - STOP_LINE) * 0.5) / TICKS_PER_SECOND);
    if remaining < BRAKING {
        approach = 0.0;
    }
    c.target_speed = ((id & 7) as f32 * 0.5) / TICKS_PER_SECOND + base;
    if !aggressive {
        let light = if next_turn == -1 { way + LEFT_LIGHTS } else { way };
        if traffic.signals.get(junction).is_some_and(|sg| sg.lights[light as usize] <= AMBER) && c.target_speed > approach {
            c.target_speed = approach;
        }
    }
    let mut limited = false;
    if QUEUE_DISTANCE > remaining && nxt.street != -1 {
        let end = traffic.cars[id].step(traffic.cars[id].route_index + 1).end;
        let target_lane = traffic.cars[id].target_lane;
        let list = traffic.street_cars.get(nxt.street as usize).cloned().unwrap_or_default();
        for j in list {
            if j == id || (traffic.cars[j].lane_mask >> (target_lane & 31)) & 1 == 0 {
                continue;
            }
            if aggressive {
                approach = maxss(AGGRESSIVE_MIN, approach);
            }
            let o = &traffic.cars[j];
            if !(QUEUE_REACH > o.near[(end & 1) as usize]) {
                continue;
            }
            let v = o.vel;
            let speed = (v.z * v.z + (v.x * v.x + v.y * v.y)).sqrt();
            if !(PUSHING > speed) {
                continue;
            }
            let c = &mut traffic.cars[id];
            if c.target_speed > approach {
                c.target_speed = approach;
                limited = true;
            }
        }
    }
    let c = &traffic.cars[id];
    let look = Vec3::new(co * SCAN_AHEAD + pos.x, pos.y + 0.0, pos.z + SCAN_AHEAD * s);
    let ps = c.physical_street;
    let mut ahead = -1;
    for (k, o) in map.streets.iter().enumerate() {
        if k as i32 == ps {
            continue;
        }
        if look.x >= o.bounds_min.x && o.bounds_max.x >= look.x && look.z >= o.bounds_min.z && o.bounds_max.z >= look.z && look.y >= o.bounds_min.y && o.bounds_max.y >= look.y {
            ahead = k as i32;
        }
    }
    let tail;
    'clear: {
        if ps != -1 {
            let sd = st.dir;
            let dot = ((co * sd.x) + sd.y * 0.0) + s * sd.z;
            let mask = (1 << (c.next_lane & 31)) | c.lane_mask;
            let mut clr = lane_clearance(traffic, map, id, ps, mask, 0.0 > dot, c.front_proj);
            if aggressive {
                clr = maxss(AGGRESSIVE_MIN, clr);
            }
            let c = &mut traffic.cars[id];
            if c.target_speed > clr {
                c.target_speed = clr;
                limited = true;
                if ahead == -1 {
                    if !aggressive {
                        return;
                    }
                    tail = true;
                    break 'clear;
                }
            }
        }
        if ahead != -1 {
            let o = &map.streets[ahead as usize];
            let d2 = o.dir;
            let w = map.intersections[o.intersections[0]].world_pos;
            let dot = (co * d2.x + d2.y * 0.0) + s * d2.z;
            let c = &traffic.cars[id];
            let (mn, mx) = (c.bounds_min, c.bounds_max);
            let pmax = ((mx.x - w.x) * d2.x + (mx.y - w.y) * d2.y) + (mx.z - w.z) * d2.z;
            let pmin = ((mn.x - w.x) * d2.x + d2.y * (mn.y - w.y)) + d2.z * (mn.z - w.z);
            let p = if dot > 0.0 { maxss(pmin, pmax) } else { minss(pmin, pmax) };
            let clr = lane_clearance(traffic, map, id, ahead, 1 << (c.next_lane & 31), 0.0 > dot, p);
            let c = &mut traffic.cars[id];
            if !aggressive {
                if c.target_speed > clr {
                    c.target_speed = clr;
                }
                return;
            }
            let clr = maxss(AGGRESSIVE_MIN, clr);
            if c.target_speed > clr {
                c.target_speed = clr;
                tail = true;
                break 'clear;
            }
        } else if !aggressive {
            return;
        }
        tail = limited;
    }
    let c = &mut traffic.cars[id];
    if tail && CRAWL > c.target_speed && c.next_lane == lane {
        let mut nl = c.next_lane + if crate::rng::rand() & 1 != 0 { 1 } else { -1 };
        if nl < 0 {
            nl = 0;
        }
        if !(st.lanes.len() as i32 > nl) {
            nl = st.lanes.len() as i32 - 1;
        }
        c.next_lane = nl;
    }
    if MIN_SPEED > c.target_speed {
        c.target_speed = MIN_SPEED;
    }
}

/// The intersection part of ai_traffic_car: a curve from the lane's end to the next street's lane, slower for a turn
/// and with the steering, taking the next street once its lane's start is passed.
fn cross_intersection(traffic: &mut Traffic, map: &StreetMap, id: usize) {
    let c = &traffic.cars[id];
    if c.intersection == -1 {
        return;
    }
    let ri = c.route_index;
    let (cur, nxt) = (c.step(ri), c.step(ri + 1));
    let st = &map.streets[cur.street as usize];
    let l0 = lane_at(st, c.lane);
    let p0 = if cur.end != 0 { l0.start } else { l0.end };
    let Some(nst) = map.streets.get(nxt.street as usize) else { return };
    let l1 = lane_at(nst, c.target_lane);
    let p1 = if nxt.end != 0 { l1.end } else { l1.start };
    let (ds, dn) = (st.dir, nst.dir);
    let delta = Vec3::new(p1.x - p0.x, p1.y - p0.y, p1.z - p0.z);
    let mut a = (((delta.y * ds.y) + (ds.x * delta.x)) + ds.z * delta.z).abs();
    let b = (((dn.x * ds.x) + (dn.y * ds.y)) + dn.z * ds.z).abs();
    if b > SHARP {
        a *= 0.5;
    }
    if cur.end != 0 {
        a = -a;
    }
    let ctrl = Vec3::new(ds.x * a + p0.x, ds.y * a + p0.y, a * ds.z + p0.z);
    let len = ((delta.x * delta.x + 0.0) + delta.z * delta.z).sqrt();
    let u = if len == 0.0 {
        Vec3::ZERO
    } else {
        let inv = 1.0 / len;
        Vec3::new(delta.x * inv, inv * 0.0, delta.z * inv)
    };
    let (s, co) = sincos(c.yaw);
    let pos = c.pos;
    let (t, om, q0, q1) = 'bez: {
        let clamped_end = (1.0, 0.0, Vec3::new((p0.x * 0.0) * 0.0, (p0.y * 0.0) * 0.0, (p0.z * 0.0) * 0.0), p1);
        if !(len > BEZIER_EPSILON) {
            break 'bez clamped_end;
        }
        let w = c.wheelbase + NEXT_LOOK;
        let t = ((((w * 0.0 + pos.y) - p0.y) * u.y + ((co * w + pos.x) - p0.x) * u.x) + ((s * w + pos.z) - p0.z) * u.z) / len + CORNER_BIAS;
        if 0.0 > t {
            (0.0, 1.0, p0, Vec3::new((p1.x * 0.0) * 0.0, (p1.y * 0.0) * 0.0, (p1.z * 0.0) * 0.0))
        } else if t > 1.0 {
            clamped_end
        } else {
            let om = 1.0 - t;
            (t, om, Vec3::new((p0.x * om) * om, (p0.y * om) * om, (p0.z * om) * om), Vec3::new((p1.x * t) * t, (p1.y * t) * t, (p1.z * t) * t))
        }
    };
    let bx = ((q0.x + ((ctrl.x + ctrl.x) * om) * t) + q1.x) - pos.x;
    let by = ((q0.y + ((ctrl.y + ctrl.y) * om) * t) + q1.y) - pos.y;
    let bz = ((q0.z + ((ctrl.z + ctrl.z) * om) * t) + q1.z) - pos.z;
    let dist = (bz * bz + (by * by + bx * bx)).sqrt();
    let target_yaw = if dist > BEZIER_EPSILON { (bz as f64).atan2(bx as f64) as f32 } else { c.yaw };
    let want = steer_toward(c.yaw, target_yaw, TURN_STEER, TURN_STEER_MAX);
    let mut dsteer = want - c.steer;
    let factor = if -TURN_STEER_RATE > dsteer {
        dsteer = -TURN_STEER_RATE;
        0.5
    } else if dsteer > TURN_STEER_RATE {
        dsteer = TURN_STEER_RATE;
        0.5
    } else {
        let h = 0.5 - dsteer.abs() as f64;
        h + h
    };
    let aggressive = c.is_aggressive != 0;
    let k = c.step(ri + 1).turn;
    let mut base = SLOW as f64;
    if k == 0 {
        base = CRUISE as f64;
        if aggressive {
            base = 0.4000000059604645;
            if traffic.coop {
                base = coop_speed(traffic, 4) as f64;
            }
        }
    }
    let c = &mut traffic.cars[id];
    c.steer = dsteer + c.steer;
    c.target_speed = (factor * base) as f32;
    let dn2 = if c.step(ri + 1).end != 0 { neg(dn) } else { dn };
    let qx = (p1.x - (dn2.x * NEXT_LOOK + pos.x)) * dn2.x;
    let qz = (p1.z - (NEXT_LOOK * dn2.z + pos.z)) * dn2.z;
    let q = qz + (0.0 * dn2.y + qx);
    if 1.0 > q {
        if ri < c.route_len - 1 {
            c.route_index = ri + 1;
            c.intersection = -1;
            c.street = nxt.street;
            c.lane = c.target_lane;
            c.next_lane = c.target_lane;
        } else {
            c.intersection = -1;
            c.street = cur.street;
            crate::rng::rand();
            let n = map.streets.len() as i32;
            if cur.street == LOOP_STREET {
                if aggressive && n != 0 {
                    random_street(map);
                }
            } else {
                let mut dest = LOOP_STREET;
                if aggressive {
                    dest = n;
                    if n != 0 {
                        dest = random_street(map);
                    }
                }
                if !((cur.street - LOOP_STREET) as u32 <= 1) {
                    let r = rand_bit();
                    let c = &mut traffic.cars[id];
                    let e = c.step(c.route_index).end;
                    plan_route(c, map, cur.street, e, dest, r);
                }
            }
        }
    }
    if nxt.street == -1 {
        return;
    }
    let c = &traffic.cars[id];
    let e2 = c.step(c.route_index + 1).end;
    let w = map.intersections[nst.intersections[0]].world_pos;
    let (mn, mx) = (c.bounds_min, c.bounds_max);
    let pmax = ((mx.x - w.x) * dn.x + (mx.y - w.y) * dn.y) + (mx.z - w.z) * dn.z;
    let pmin = ((mn.x - w.x) * dn.x + (mn.y - w.y) * dn.y) + (mn.z - w.z) * dn.z;
    let p = if e2 != 0 { if pmax > pmin { pmin } else { pmax } } else { maxss(pmin, pmax) };
    let clr = lane_clearance(traffic, map, id, nxt.street, 1 << (c.target_lane & 31), e2 != 0, p);
    let c = &mut traffic.cars[id];
    if c.target_speed > clr {
        c.target_speed = clr;
    }
}
