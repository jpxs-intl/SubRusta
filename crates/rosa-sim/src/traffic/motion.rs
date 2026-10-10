use glam::Vec3;
use rosa_physics::rotation::{IDENTITY, rotate_orientation};

use super::{DRIVEN, Traffic};
use crate::{
    vehicle::types::VehicleType,
    world::{map::Map, streets::StreetMap, trace::line_intersect_level},
};

const CYCLE: i32 = 800;
const AMBER_AT: i32 = 671;
const GREEN: i32 = 2;
const AMBER: i32 = 1;
/// Away from players the car speeds up by at most 1/720 and slows by at most 1/180 a tick.
const ACCEL_GAIN: f32 = 16.0;
const ACCEL_MAX: f32 = f32::from_bits(0x3ab6_0b61);
const ACCEL_SCALE: f32 = 5.0;
const BRAKE_GAIN: f32 = 32.0;
const BRAKE_MAX: f32 = f32::from_bits(0xbbb6_0b61);
const BRAKE_SCALE: f32 = 10.0;
const BRAKE_LIMIT: f32 = 2.0;
const TICKS2: f32 = 3600.0;
const PARK_TARGET: f32 = f32::from_bits(0x3d08_8889);
const PARK_SPEED: f32 = f32::from_bits(0x3c88_8889);
/// The ground under a virtual car is traced from 2 above to 32 below, and it rides 0.875 of a wheel up.
const GROUND_DEPTH: f32 = -32.0;
const WHEEL_RIDE: f32 = 0.875;

/// Each intersection's lights: every 800 ticks the green moves to the next way in, skipping missing streets,
/// with amber for the last 128; a way in with no street to its right gets the opposite way's green instead of a
/// left arrow, and dead ends and corners stay green.
pub fn update_signals(traffic: &mut Traffic, map: &StreetMap) {
    for (sg, i) in traffic.signals.iter_mut().zip(&map.intersections) {
        sg.timer += 1;
        sg.cycle = CYCLE;
        if sg.timer > CYCLE - 1 {
            sg.phase = (sg.phase + 1) & 3;
            if i.streets[sg.phase as usize] == -1 {
                sg.phase = (sg.phase + 1) & 3;
                if i.streets[sg.phase as usize] == -1 {
                    sg.phase = (sg.phase + 1) & 3;
                }
            }
            sg.timer = 0;
        }
        sg.lights = [0; 8];
        let p = sg.phase as usize;
        sg.lights[p] = GREEN;
        let second = if i.streets[(p + 1) & 3] != -1 { p + 4 } else { (p + 2) & 3 };
        sg.lights[second] = GREEN;
        if sg.timer > AMBER_AT {
            sg.lights[p] = AMBER;
            sg.lights[second] = AMBER;
        }
        if i.streets.iter().filter(|&&s| s == -1).count() > 1 {
            sg.lights = [GREEN; 8];
        }
    }
}

/// A car away from players: it turns with its steering, keeps its speed along its heading, speeds up or slows
/// towards its target, and follows the ground, pitched to it over the wheelbase.
pub fn move_virtual_car(traffic: &mut Traffic, map: &Map, types: &[VehicleType], id: usize) {
    let (radius, wheel_y) = types.get(traffic.cars[id].kind as usize).and_then(|t| t.wheels.first()).map_or((0.0, 0.0), |w| (w.radius, w.local_pos.y));
    let c = &mut traffic.cars[id];
    if c.is_bot != DRIVEN {
        c.target_speed = 0.0;
    }
    let v = c.vel;
    let speed = ((v.y * v.y + v.x * v.x) + v.z * v.z).sqrt();
    let st = c.steer;
    let mut turn = 0.0;
    if st.abs() > 0.0 {
        let mut r = ((c.wheelbase as f64) / (st.abs() as f64).tan()) as f32;
        if 0.0 > st {
            r = -r;
        }
        turn = speed / r;
    }
    let y = turn + c.yaw;
    let yd = y as f64;
    c.yaw = if -180.0_f64.to_radians() > yd {
        (yd + 360.0_f64.to_radians()) as f32
    } else if yd > 180.0_f64.to_radians() {
        (yd - 360.0_f64.to_radians()) as f32
    } else {
        y
    };
    let (s, co) = (c.yaw as f64).sin_cos();
    let (s, co) = (s as f32, co as f32);
    let proj = (v.x * co + 0.0 * v.y) + v.z * s;
    let (mut vx, mut vy, mut vz) = (co * proj, 0.0 * proj, proj * s);
    let along = (co * vx + 0.0 * vy) + s * vz;
    let target = c.target_speed;
    if target > along {
        let d = (target - along) * ACCEL_GAIN;
        let (a, lift) = if d > 1.0 { (ACCEL_MAX, 0.0) } else {
            let a = (d * ACCEL_SCALE) / TICKS2;
            (a, 0.0 * a)
        };
        vx += co * a;
        vy += lift;
        vz += a * s;
    } else {
        let d = (along - target) * BRAKE_GAIN;
        let a = if d > BRAKE_LIMIT { BRAKE_MAX } else {
            let a = (-d * BRAKE_SCALE) / TICKS2;
            vy += 0.0 * a;
            a
        };
        vx += co * a;
        vz += s * a;
        if PARK_TARGET > target && PARK_SPEED > ((vx * vx + vy * vy) + vz * vz).sqrt() {
            (vx, vy, vz) = (0.0, 0.0, 0.0);
        }
    }
    c.vel = Vec3::new(vx, vy, vz);
    c.pos = Vec3::new(vx + c.pos.x, vy + c.pos.y, vz + c.pos.z);
    let ride = WHEEL_RIDE * radius - wheel_y;
    let up = Vec3::Y;
    let p = c.pos;
    let start = Vec3::new((up.x + up.x) + p.x, (up.y + up.y) + p.y, (up.z + up.z) + p.z);
    let end = Vec3::new(p.x + up.x * GROUND_DEPTH, up.y * GROUND_DEPTH + p.y, up.z * GROUND_DEPTH + p.z);
    if let Some(h) = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, start, end) {
        c.pos.y = ride + h.hit.pos.y;
    }
    let mut rot = IDENTITY;
    let axis = rot[1];
    rotate_orientation(&mut rot, axis, 90.0_f32.to_radians());
    let axis = rot[1];
    rotate_orientation(&mut rot, axis, c.yaw);
    let back = -c.wheelbase;
    let r2 = rot[2];
    let p = c.pos;
    let rear = Vec3::new(r2.x * back + p.x, r2.y * back + p.y, p.z + back * r2.z);
    let end = Vec3::new(rear.x + up.x * GROUND_DEPTH, rear.y + up.y * GROUND_DEPTH, GROUND_DEPTH * up.z + rear.z);
    if let Some(h) = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, rear, end) {
        let hy = ride + h.hit.pos.y;
        let pitch = -((((hy - p.y) as f64).atan2(c.wheelbase as f64)) as f32);
        let axis = rot[0];
    rotate_orientation(&mut rot, axis, pitch);
    }
    c.rot = rot;
}
