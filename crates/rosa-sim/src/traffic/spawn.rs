use glam::Vec3;
use rosa_physics::rotation::{IDENTITY, rotate_orientation};
use rosa_protocol::GameMode;

use super::{DRIVEN, HALF_PI, HIDDEN_STREET, LOOP_STREET, MAX_ROUTE, PARKED, PI, RouteStep, Traffic, TrafficCar, maxss, rand_bit, rand_mod, route::plan_route};
use crate::{vehicle::types::VehicleType, world::map::Map};

const TOWN_CAR: usize = 0;
const VAN: usize = 7;
const MINIVAN: usize = 9;
const HATCHBACK: usize = 15;
const COLORS: i32 = 6;
const PROGRESS_STEP: f32 = 0.00390625;
/// The ground is found by tracing 128 down from two above the street's higher end.
const TRACE_LIFT: f32 = 2.0;
const TRACE_DEPTH: f32 = -128.0;
const RIDE_HEIGHT: f32 = 0.375;
const FRONT: f32 = 3.5;
const REAR: f32 = -0.5;
const WHEELBASE: f32 = 3.0;
/// The eliminator spawns some cars parked in a third lane of the flat three lane streets.
const PARKING_LANE: i32 = 2;

/// The traffic's vehicle types by a roll of 0 to 15: town cars most often, then vans and minivans, and a few
/// hatchbacks.
fn car_kind(roll: u32) -> usize {
    match roll {
        0..=2 => VAN,
        3..=8 => TOWN_CAR,
        9..=12 => MINIVAN,
        13 => VAN,
        _ => HATCHBACK,
    }
}

/// create_traffic3: `count` cars on random streets, ends and lanes, each partway along its lane.
pub fn create_traffic(traffic: &mut Traffic, map: &Map, types: &[VehicleType], mode: GameMode, count: i32) {
    let streets = &map.streets;
    let n = streets.streets.len() as i32;
    for _ in 0..count {
        let slot = rand_bit();
        let mut street = rand_mod(n);
        if traffic.roundcity {
            while street == HIDDEN_STREET || street == LOOP_STREET {
                street = rand_mod(n);
            }
        }
        let mut lane = rand_bit();
        let st = &streets.streets[street as usize];
        let flat_three = mode == GameMode::Eliminator && streets.intersections[st.intersections[0]].pos.y == streets.intersections[st.intersections[1]].pos.y && st.left_lanes == 3;
        if flat_three {
            lane = rand_mod(3);
        }
        let kind = car_kind(crate::rng::rand() & 15);
        let byte = crate::rng::rand() & 0xff;
        let color = rand_mod(COLORS);
        let id = create_traffic_car(traffic, map, types, kind, color, street, slot, lane, byte as f32 * PROGRESS_STEP);
        if lane == PARKING_LANE {
            traffic.cars[id].is_bot = PARKED;
        }
    }
}

/// create_traffic_car: a car on a street's lane `lane` from the middle (towards its first intersection's side for
/// slot 0), `progress` of the way along, on the ground, with a route to a random street.
#[allow(clippy::too_many_arguments)]
pub fn create_traffic_car(traffic: &mut Traffic, map: &Map, types: &[VehicleType], kind: usize, color: i32, street: i32, slot: i32, lane: i32, progress: f32) -> usize {
    let streets = &map.streets;
    let st = &streets.streets[street as usize];
    let n = st.lanes.len() as i32;
    let mut k = (n >> 1) - lane - 1;
    if k < 0 {
        k = 0;
    }
    if n <= k {
        k = n - 1;
    }
    let mut dir = st.dir;
    let (from, dx, dz) = if slot == 0 {
        let l = st.lanes[(n - k - 1) as usize];
        (l.start, l.end.x - l.start.x, l.end.z - l.start.z)
    } else {
        dir = Vec3::new(-dir.x, -dir.y, -dir.z);
        let l = st.lanes[k as usize];
        (l.end, l.start.x - l.end.x, l.start.z - l.end.z)
    };
    let len = (dz * dz + (dx * dx + 0.0)).sqrt();
    let p = progress * len;
    let mut yaw = if st.north_south != 0 { -std::f32::consts::FRAC_PI_2 } else { -std::f32::consts::PI };
    if slot == 0 {
        yaw = (yaw as f64 + PI) as f32;
    }
    let pos = Vec3::new(dir.x * p + from.x, dir.y * p + from.y, dir.z * p + from.z);
    let ends = st.intersections.map(|i| streets.intersections[i].world_pos.y);
    let top = maxss(ends[1], maxss(ends[0], pos.y)) + TRACE_LIFT;
    let up = Vec3::Y;
    let start = Vec3::new(pos.x, top, pos.z);
    let end = Vec3::new(pos.x + up.x * TRACE_DEPTH, top + up.y * TRACE_DEPTH, TRACE_DEPTH * up.z + pos.z);
    let ground = crate::world::trace::line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, start, end).map_or(top, |h| h.hit.pos.y);
    let wheel_y = types.get(kind).and_then(|t| t.wheels.first()).map_or(0.0, |w| w.local_pos.y);
    let left = st.left_lanes;
    let car_lane = if slot != 0 { left - 1 - lane } else { lane + left };
    let id = traffic.cars.len();
    let mut car = TrafficCar {
        kind,
        human: -1,
        vehicle: -1,
        state: 0,
        pos: Vec3::new(pos.x, (RIDE_HEIGHT - wheel_y) + ground, pos.z),
        vel: Vec3::ZERO,
        yaw,
        rot: IDENTITY,
        steer: 0.0,
        target_speed: 0.0,
        front: FRONT,
        rear: REAR,
        wheelbase: WHEELBASE,
        bounds_min: Vec3::ZERO,
        bounds_max: Vec3::ZERO,
        is_bot: DRIVEN,
        is_aggressive: 0,
        route_index: 0,
        route_len: 0,
        route: [RouteStep::default(); MAX_ROUTE],
        street,
        slot,
        intersection: -1,
        lane: car_lane,
        next_lane: car_lane,
        target_lane: 0,
        lane_change: 0.0,
        stuck: 0,
        physical_street: 0,
        physical_intersection: 0,
        front_proj: 0.0,
        rear_proj: 0.0,
        far: [0.0; 2],
        near: [0.0; 2],
        lane_mask: 0,
        color,
    };
    let to = rand_mod(streets.streets.len() as i32);
    let to_slot = rand_bit();
    plan_route(&mut car, streets, street, slot, to, to_slot);
    let mut rot = IDENTITY;
    let axis = rot[1];
        rotate_orientation(&mut rot, axis, (yaw as f64 + HALF_PI) as f32);
    car.rot = rot;
    traffic.cars.push(car);
    if traffic.generations.len() <= id {
        traffic.generations.resize(id + 1, 0);
    }
    traffic.generations[id] += 1;
    id
}
