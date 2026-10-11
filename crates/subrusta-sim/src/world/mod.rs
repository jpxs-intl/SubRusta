use crate::world::map::Map;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
#[allow(unused)]
pub enum Weekday {
    Sunday = 0,
    Monday = 1,
    Tuesday = 2,
    Wednesday = 3,
    Thursday = 4,
    Friday = 5,
    Saturday = 6,
}

pub mod area;
pub mod blocks;
pub mod building;
pub mod capsule;
pub mod city;
pub mod city_objects;
pub mod collide;
pub mod ground;
pub mod item_sets;
pub mod level;
pub mod map;
pub mod mesh;
pub mod meshes;
pub mod roads;
pub mod sphere;
pub mod sphere_cast;
pub mod streets;
pub mod trace;
pub mod track;

pub struct World {
    sun_angle: u16,
    sun_axial_tilt: u16,
    pub weekday: Weekday,
    pub map: Map,
}

impl World {
    pub(crate) fn new(map_name: String) -> Self {
        World { sun_angle: 1000, sun_axial_tilt: 1000, weekday: Weekday::Monday, map: Map::load_map(map_name).unwrap() }
    }

    pub(crate) fn sun_angle(&self) -> u16 {
        self.sun_angle
    }
    pub(crate) fn sun_axial_tilt(&self) -> u16 {
        self.sun_axial_tilt
    }

    /// The sun's angle and axial tilt (game_mode_state +0x2b4, +0x2b8), as the initial sync sends them: 16-bit angles,
    /// the top bit for negative.
    pub(crate) fn set_sun(&mut self, angle: f32, tilt: f32) {
        self.sun_angle = encode_angle(angle);
        self.sun_axial_tilt = encode_angle(tilt);
    }
}

/// The 16-bit angle encoding of server_send (0x426580): the size as a share of the turn, the top bit for negative.
fn encode_angle(v: f32) -> u16 {
    const HALF: i64 = 1 << 15;
    let turn = 6.28318530718;
    let n = ((HALF as f64 / turn) * v.abs() as f64) as i64 & (HALF - 1);
    (if 0.0 > v { HALF | n } else { n }) as u16
}
