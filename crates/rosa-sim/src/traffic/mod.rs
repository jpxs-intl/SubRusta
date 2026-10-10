use glam::Vec3;
use rosa_physics::RotMatrix;

use crate::world::streets::StreetMap;

pub mod ai;
pub mod grid;
pub mod motion;
pub mod route;
pub mod spawn;

/// The route steps a car's record holds (+0x8c, 0x14 each, up to +0x584).
pub const MAX_ROUTE: usize = 63;
/// The streets the Round City traffic hack keeps cars off and treats as one loop.
pub const LOOP_STREET: i32 = 0x43;
pub const LOOP_STREET_2: i32 = 0x44;
pub const HIDDEN_STREET: i32 = 0x17;
/// Each street lists at most this many of the cars on it (+0x22c count, +0x230).
pub const MAX_STREET_CARS: usize = 256;

/// is_bot: driven by the traffic, parked, or taken over by a player.
pub const PARKED: i32 = 0;
pub const DRIVEN: i32 = 1;
pub const TAKEN: i32 = 2;

/// One step of a planned route: the street, the end it is driven from (0 from its first intersection) and the turn onto
/// it (-1 left, 1 right).
#[derive(Clone, Copy, Debug, Default)]
pub struct RouteStep {
    pub street: i32,
    pub end: i32,
    pub turn: i32,
}

/// A traffic car (traffic_cars, 0x5fc each): a car that drives itself along the streets, simulated on its own away from
/// players and as a real vehicle near them.
#[derive(Clone, Debug)]
pub struct TrafficCar {
    pub kind: usize,
    /// The bot driving it (+0x04) and the vehicle it is while near a player (+0x08), -1 for none.
    pub human: i32,
    pub vehicle: i32,
    /// How near the nearest player is (+0x0c): 2 within range, 1 just outside it, 0 away.
    pub state: i32,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub rot: RotMatrix,
    pub steer: f32,
    pub target_speed: f32,
    /// How far the body reaches ahead of (+0x58) and behind (+0x5c) its position, and the distance between its axles
    /// (+0x60).
    pub front: f32,
    pub rear: f32,
    pub wheelbase: f32,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
    pub is_bot: i32,
    pub is_aggressive: i32,
    pub route_index: i32,
    pub route_len: i32,
    pub route: [RouteStep; MAX_ROUTE],
    /// The street being driven (+0x590, -1 while crossing an intersection), the end it was entered from (+0x594) and
    /// the intersection being crossed (+0x598).
    pub street: i32,
    pub slot: i32,
    pub intersection: i32,
    /// The lane driven (+0x59c), the lane being moved to (+0x5a0) and the lane to take on the next street (+0x5a4).
    pub lane: i32,
    pub next_lane: i32,
    pub target_lane: i32,
    /// How far the move to `next_lane` has got (+0x5ac), 0 to 1.
    pub lane_change: f32,
    /// Ticks spent stopped while meaning to move (+0x5b0); past 179 the car backs out.
    pub stuck: i32,
    /// The street and intersection the car's position is in (+0x5b4, +0x5b8).
    pub physical_street: i32,
    pub physical_intersection: i32,
    /// The car's front (+0x5bc) and back (+0x5c0) along its street from the street's first intersection.
    pub front_proj: f32,
    pub rear_proj: f32,
    /// How far the car reaches from each end of its street, farthest (+0x5c4, +0x5c8) and nearest (+0x5cc, +0x5d0).
    pub far: [f32; 2],
    pub near: [f32; 2],
    /// The lanes of its street the car covers (+0x5d4).
    pub lane_mask: i32,
    pub color: i32,
}

impl TrafficCar {
    pub fn step(&self, i: i32) -> RouteStep {
        usize::try_from(i).ok().and_then(|i| self.route.get(i)).copied().unwrap_or_default()
    }

    pub fn speed(&self) -> f32 {
        ((self.vel.x * self.vel.x + self.vel.y * self.vel.y) + self.vel.z * self.vel.z).sqrt()
    }
}

/// An intersection's traffic lights (+0x44 phase, +0x48 timer, +0x4c cycle, +0x50 lights): straight on and right for
/// each way in (0 to 3) and left (4 to 7), 2 green, 1 amber, 0 red.
#[derive(Clone, Copy, Debug, Default)]
pub struct Signals {
    pub phase: i32,
    pub timer: i32,
    pub cycle: i32,
    pub lights: [i32; 8],
}

/// The traffic: its cars, the cars on each street and the intersections' lights.
#[derive(Clone, Debug, Default)]
pub struct Traffic {
    pub cars: Vec<TrafficCar>,
    /// How many cars have been made in each slot (raw 0x2ed48c20).
    pub generations: Vec<i32>,
    pub street_cars: Vec<Vec<usize>>,
    pub signals: Vec<Signals>,
    /// hack_roundcity_traffic: on Round City, cars keep off two streets and treat the loop as one.
    pub roundcity: bool,
    /// The co-op wave (0x44f87324), which speeds up aggressive cars.
    pub coop_level: i32,
    pub coop: bool,
}

impl Traffic {
    pub fn new(map: &StreetMap, roundcity: bool) -> Self {
        Traffic { street_cars: vec![Vec::new(); map.streets.len()], signals: vec![Signals::default(); map.intersections.len()], roundcity, ..Default::default() }
    }

    /// The part of reset_game that empties the traffic.
    pub fn clear(&mut self) {
        self.cars.clear();
        self.generations.clear();
        self.street_cars.iter_mut().for_each(Vec::clear);
    }
}

pub(crate) fn maxss(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

pub(crate) fn minss(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

/// rand() % n as the binary's idiv does it.
pub(crate) fn rand_mod(n: i32) -> i32 {
    (crate::rng::rand() as i32).wrapping_rem(n)
}

pub(crate) fn rand_bit() -> i32 {
    (crate::rng::rand() & 1) as i32
}

/// get_random_street: a street with at most three lanes each way, rerolled up to 1024 times.
pub fn random_street(map: &StreetMap) -> i32 {
    let n = map.streets.len() as i32;
    let wide = |s: i32| map.streets.get(s as usize).is_some_and(|st| st.left_lanes > 3 || st.right_lanes > 3);
    let mut r = rand_mod(n);
    for _ in 0..0x400 {
        if !wide(r) {
            break;
        }
        r = rand_mod(n);
    }
    r
}

pub(crate) fn lane_count(map: &StreetMap, s: i32) -> i32 {
    map.streets.get(s as usize).map_or(0, |st| st.lanes.len() as i32)
}

pub(crate) fn left_lanes(map: &StreetMap, s: i32) -> i32 {
    map.streets.get(s as usize).map_or(0, |st| st.left_lanes)
}
