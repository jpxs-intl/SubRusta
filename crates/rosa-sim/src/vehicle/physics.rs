use glam::Vec3;
use rosa_protocol::clientbound::game::VehicleKind;
use rosa_physics::{
    RigidBodies, RigidBody, RotMatrix, Table,
    bond::Bond,
    rotation::{multiply_matrixes, quaternion_to_rotation_vector, rot_matrix_to_quaternion, rotate_vector_about_axis},
};
use rosa_protocol::clientbound::game::events::sound::Sound;

use super::{Vehicle, types::VehicleType};
use crate::{
    world::{
        capsule::{capsule_intersect_triangle, capsule_intersect_triangle_facing},
        collide::{calculate_face_normal, point_in_triangle_oriented, segment_intersect_face},
        map::Map,
        sphere::{segment_intersect_plane_two_sided, segment_intersect_sphere, sphere_intersect_level},
        trace::line_intersect_level,
    },
};

const WATER_LEVEL: f32 = 23.0;
const FALLEN: f32 = -32.0;
const DRIVEN: i32 = 1;
const ROTOR: i32 = 2;
const UNCONTROLLED: i32 = -1;
const CLUTCH_HELD: i32 = 2;
const HANDBRAKE: i32 = 4;
const HUGE: f32 = 65536.0;
const TICKS_PER_SECOND: f32 = 60.0;
const BLOCK_SCALE: f32 = 0.25;
const DAMAGE_SPEED: f32 = 7.0 / 60.0;
const CRASH_SPEED: f32 = 1.0 / 30.0;
const SHIFT_DELAY: i32 = 30;
const MAX_RPM: i32 = 0x1fff;
const TINY: f32 = 1.0 / 65536.0;
const QUARTER_TURN: f64 = 90.0_f64.to_radians();
const FULL_TURN: f64 = 360.0_f64.to_radians();
const GEAR_RATIOS: [(i32, f32); 5] = [(1, 10.849999), (2, 7.7000003), (3, 5.25), (4, 3.5), (-1, -10.5)];
const WHEEL_FRICTION: f32 = 1.2;
const WHEEL_DEPTH_SCALE: f32 = 0.03125;
const WHEEL_SOFTNESS: f32 = 0.125;
const CHASSIS_FRICTION: f32 = 0.6;
const MAX_CHASSIS_DEPTH: f32 = 0.125;
/// A vehicle in this controllable state is out of play: not sent to clients and not collided with.
const HIDDEN: i32 = 3;
const VEHICLE_FRICTION: f32 = 0.4;
const VEHICLE_DEPTH_SCALE: f32 = 1.0 / 64.0;
const VEHICLE_SOFTNESS: f32 = 1.0 / 32.0;

/// What a vehicle step asks of the sim.
pub enum VehicleOutput {
    /// A sound at the vehicle (event 9).
    Sound { sound: Sound, pos: Vec3, volume: f32, pitch: f32 },
    /// vehicle_take_damage from a crash.
    Damage { vehicle: usize, amount: i32 },
    /// An update_vehicle event (4) about the vehicle.
    Update { vehicle: usize, kind: i32, part: i32, pos: Vec3, vel: Vec3 },
}

/// The record part of vehicle_update_bbox_wake_items: the vehicle follows its chassis body (its position the body
/// centre less the centroid offset turned by the body), and its bounds cover its render shape.
pub fn update_vehicle_bounds(v: &mut Vehicle, bodies: &RigidBodies, types: &[VehicleType]) {
    {
        let Some(b) = bodies.get(v.body) else { return };
        let t = &types[v.kind as usize];
        v.prev_pos = v.pos;
        v.prev_rot = v.rot;
        let c = t.centroid_offset;
        let [r0, r1, r2] = b.rot;
        let p = b.pos;
        v.pos = c.x * r0 + p + c.y * r1 + c.z * r2;
        v.rot = b.rot;
        v.vel = b.vel;
        v.ang_vel = b.ang_vel;
        let (pos, [r0, r1, r2]) = (v.pos, v.rot);
        let (mut mn, mut mx) = ([HUGE; 3], [-HUGE; 3]);
        for q in &t.render_verts {
            let w = [
                ((q.x * r0.x + pos.x) + q.y * r1.x) + q.z * r2.x,
                ((r0.y * q.x + pos.y) + r1.y * q.y) + r2.y * q.z,
                ((r0.z * q.x + pos.z) + r1.z * q.y) + q.z * r2.z,
            ];
            for k in 0..3 {
                if !(mn[k] <= w[k]) {
                    mn[k] = w[k];
                }
                if !(w[k] <= mx[k]) {
                    mx[k] = w[k];
                }
            }
        }
        v.bounds_min = Vec3::from_array(mn);
        v.bounds_max = Vec3::from_array(mx);
        v.block_min = mn.map(|x| (x * BLOCK_SCALE) as i32);
        v.block_max = mx.map(|x| (x * BLOCK_SCALE) as i32);
    }
}

fn crash_sound(v: &Vehicle, out: &mut Vec<VehicleOutput>) {
    let r = rand::random_range(0..256) as f32;
    out.push(VehicleOutput::Sound { sound: Sound::CarCrash, pos: v.pos, volume: v.crash * 0.5, pitch: r * (1.0 / 1024.0) + 0.875 });
}

/// The first pass of vehicleSimulation: a hard change of velocity damages the vehicle and builds up crash sounds,
/// and the chassis is slowed along each of its axes (harder under water), never by more than an eighth of its
/// speed.
fn crash_and_drag(id: usize, v: &mut Vehicle, bodies: &mut RigidBodies, out: &mut Vec<VehicleOutput>) {
    let (pv, vel) = (v.prev_vel, v.vel);
    let d = pv - vel;
    let dv = d.length();
    v.prev_vel = vel;
    if FALLEN > v.pos.y {
        v.despawn_time = 0;
        v.spawned_state = 0;
    }
    if v.kind != VehicleKind::NoClip {
        if dv > DAMAGE_SPEED {
            out.push(VehicleOutput::Damage { vehicle: id, amount: (dv * TICKS_PER_SECOND + 10.0) as i32 });
        }
        if dv > CRASH_SPEED {
            v.crash += dv * TICKS_PER_SECOND * 0.125;
            if v.crash > 2.0 {
                crash_sound(v, out);
                v.crash -= 2.0;
            }
        } else {
            if v.crash > 0.5 {
                crash_sound(v, out);
            }
            v.crash = 0.0;
        }
    }
    let Some(b) = bodies.get_mut(v.body) else { return };
    let vel = b.vel;
    let speed = vel.length();
    let dir = vel.normalize_or_zero();
    let s2 = speed * speed;
    let under = !(b.pos.y > WATER_LEVEL);
    let (hi, lo) = (speed * 0.125, -speed * 0.125);
    let clamp = |x: f32| {
        let m = if lo > x { lo } else { x };
        if hi < m { hi } else { m }
    };
    let [a0, a1, a2] = b.rot;
    let d0 = a0.dot(dir);
    let k0 = ((s2 * if under { 800.0 } else { 4.0 }) * 0.5) * 0.75;
    let c0 = clamp(d0 * (-k0 / 1000.0));
    let vel = a0 * c0 + vel;
    let d1 = a1.dot(dir);
    let s2x2 = s2 + s2;
    let k1 = ((s2x2 * if under { 800.0 } else { 4.0 }) * 0.5) * 0.75;
    let c1 = clamp((-k1 / 1000.0) * d1);
    let vel = vel + a1 * c1;
    let d2 = dir.dot(a2);
    let k2 = if under { s2 * 400.0 * 0.25 } else { s2x2 * 0.25 };
    let c2 = clamp(d2 * (-(0.75 * k2) / 1000.0));
    b.vel = vel + a2 * c2;
}

fn gear_shift_sound(v: &Vehicle, out: &mut Vec<VehicleOutput>) {
    let (volume, pitch) = if v.gear == 0 { (0.75, 1.0) } else { (1.0, 0.75) };
    out.push(VehicleOutput::Sound { sound: Sound::GearShift, pos: v.pos, volume, pitch });
}

/// vehicle_update_car_drivetrain: the driver's controls applied: the throttle, the clutch, the gear from the
/// stick (the far right position shifting automatically by engine speed), the brake and handbrake, the
/// front wheels steered (the inner one more, after Ackermann) and the engine's revolutions per minute for the
/// clients.
fn update_car_drivetrain(v: &mut Vehicle, out: &mut Vec<VehicleOutput>) {
    v.throttle = if v.gas_control > 0.0 {
        v.gas_control.abs()
    } else if v.throttle > 0.125 {
        v.throttle - 0.125
    } else {
        0.0
    };
    let flags = v.controls;
    v.auto_shift = 0;
    v.clutch = if flags & CLUTCH_HELD == 0 {
        if 0.875 > v.clutch { v.clutch + 0.125 } else { 1.0 }
    } else if v.clutch > 0.125 {
        v.clutch - 0.125
    } else {
        0.0
    };
    let threshold = if v.kind == VehicleKind::Minivan { 450.0_f32.to_radians() } else { 600.0_f32.to_radians() };
    let uncontrolled = |v: &mut Vehicle| {
        v.engine_speed *= 0.9375;
        v.brake = 1.0;
        v.gear = 0;
    };
    let select = |v: &mut Vehicle, gear: i32, out: &mut Vec<VehicleOutput>| {
        v.gear = gear;
        v.brake = 0.0;
        if flags == UNCONTROLLED {
            uncontrolled(v);
            if v.prev_gear != 0 {
                gear_shift_sound(v, out);
            }
        } else if v.prev_gear != v.gear {
            gear_shift_sound(v, out);
        }
        match GEAR_RATIOS.iter().find(|(g, _)| *g == v.gear) {
            Some(&(_, r)) => v.gear_ratio = r,
            None if v.gear == 0 => v.gear_ratio = 0.0,
            None => {}
        }
    };
    let (gx, gy) = (v.gear_x, v.gear_y);
    if -0.5 > gy {
        if 0.5 > gx {
            select(v, 1, out);
        } else if 1.5 > gx {
            select(v, 3, out);
        } else {
            v.auto_shift = 1;
            let mut gear = v.gear;
            if gear <= 0 {
                gear = 1;
                v.gear = 1;
            }
            if v.shift_delay > 0 {
                v.shift_delay -= 1;
            } else {
                if gear != 1 && 250.0_f64.to_radians() > v.engine_speed as f64 {
                    gear -= 1;
                    v.shift_delay = SHIFT_DELAY;
                    v.gear = gear;
                }
                if gear <= 3 && v.engine_speed > threshold {
                    gear += 1;
                    v.shift_delay = SHIFT_DELAY;
                    v.gear = gear;
                }
            }
            if gear > 4 {
                v.gear = 4;
                v.brake = 0.0;
                if flags == UNCONTROLLED {
                    uncontrolled(v);
                    v.gear_ratio = 0.0;
                } else {
                    v.gear_ratio = 3.5;
                }
            } else if gear < -1 {
                v.gear = -1;
                v.brake = 0.0;
                if flags == UNCONTROLLED {
                    uncontrolled(v);
                    v.gear_ratio = 0.0;
                } else {
                    v.gear_ratio = -10.5;
                }
            } else {
                v.brake = 0.0;
                if flags == UNCONTROLLED {
                    uncontrolled(v);
                    v.gear_ratio = 0.0;
                } else {
                    let g = v.gear;
                    select(v, g, out);
                }
            }
        }
    } else if gy > 0.5 {
        if 0.5 > gx {
            select(v, 2, out);
        } else if 1.5 > gx {
            select(v, 4, out);
        } else {
            select(v, -1, out);
        }
    } else {
        v.clutch = 0.0;
        select(v, 0, out);
    }
    if 0.0 > v.gas_control {
        v.brake = -v.gas_control;
    }
    if v.controls & HANDBRAKE != 0 {
        v.brake = 2.0;
    }
    v.inv_gear_ratio = if v.gear_ratio == 0.0 { 0.0 } else { 1.0 / v.gear_ratio };
    let steer = v.steer_control;
    v.steer = steer;
    let a = steer.abs();
    let outer = if a > 0.0 {
        let t = (QUARTER_TURN - a as f64).tan();
        let y = (t * v.wheelbase as f64 + v.track as f64) as f32;
        (QUARTER_TURN - (y as f64).atan2(v.wheelbase as f64)) as f32
    } else {
        0.0
    };
    let (w0, w1) = if steer > 0.0 { (outer, a) } else { (-a, -outer) };
    if let Some(w) = v.wheels.get_mut(0) {
        w.steer = w0;
    }
    if let Some(w) = v.wheels.get_mut(1) {
        w.steer = w1;
    }
    if v.health > 0 && !v.locked {
        let rpm = ((v.engine_speed * TICKS_PER_SECOND * TICKS_PER_SECOND) as f64 / FULL_TURN) as i32;
        if rpm >= 0 {
            v.engine_rpm = rpm;
            if v.gear > 0 && rpm == 0 {
                v.engine_rpm = 1;
            } else if rpm > MAX_RPM {
                v.engine_rpm = MAX_RPM;
            }
        } else {
            v.engine_rpm = if v.gear > 0 { 1 } else { 0 };
        }
    } else {
        v.engine_speed = 0.0;
        v.engine_rpm = if v.gear > 0 { 1 } else { 0 };
    }
    if v.gas_control > 0.0 {
        v.gas_control = 0.0;
    }
    v.prev_gear = v.gear;
}

/// vehicle_wheel_collision: each side of the wheel (half its width along the axle) as a disc against the level,
/// pushed back out by a world contact.
fn wheel_collision(map: &Map, bodies: &mut RigidBodies, v: &Vehicle, k: usize) {
    let w = &v.wheels[k];
    let Some(b) = bodies.get(w.body) else { return };
    let (axle, p, r, hw) = (b.rot[0], b.pos, w.radius, w.half_width);
    let reach = r + hw;
    let tris = map.level.area.track.collect(p - reach, p + reach, crate::world::track::WALLS);
    for side in [-hw, hw] {
        let Some(p) = bodies.get(w.body).map(|b| b.pos) else { return };
        let c = axle * side + p;
        if let Some(hit) = sphere_intersect_level(&map.ground, &map.level.area, &map.level.meshes, c, axle, r) {
            bodies.add_world_contact(w.body, hit.pos - p, hit.normal, r - hit.dist, WHEEL_FRICTION, WHEEL_DEPTH_SCALE, WHEEL_SOFTNESS);
        }
        if let Some(hit) = crate::world::sphere::sphere_intersect_triangles(&tris, c, axle, r) {
            bodies.add_world_contact(w.body, hit.pos - p, hit.normal, r - hit.dist, WHEEL_FRICTION, WHEEL_DEPTH_SCALE, WHEEL_SOFTNESS);
        }
    }
}

/// The wheel part of the second pass of vehicleSimulation: each wheel's suspension travel along the chassis' up
/// axis (for the clients), its skid from the ground's friction, its turn, its collision, and its body's
/// position and velocity kept.
fn step_wheels(map: &Map, bodies: &mut RigidBodies, v: &mut Vehicle) {
    let Some(c) = bodies.get(v.body) else { return };
    let ([r0, r1, r2], pos) = (c.rot, c.pos);

    for k in 0..v.wheels.len() {
        let w = &v.wheels[k];
        let (l, h) = (w.local_pos, w.vertical_offset);

        let anchor = l.x * r0 + pos + l.y * r1 + l.z * r2 + h * r1;

        let Some(wb) = bodies.get(w.body) else { continue };
        let d = wb.pos - anchor;
        let proj = d.dot(r1);
        let w = &mut v.wheels[k];

        w.visual_height = proj.clamp(-1.0, 1.0);
        w.skid = wb.slide.clamp(0.0, 1.0);
        w.angle += w.spin;

        wheel_collision(map, bodies, v, k);

        if let Some(wb) = bodies.get(v.wheels[k].body) {
            let w = &mut v.wheels[k];

            w.world_pos = wb.pos;
            w.vel = wb.vel;
        }
    }
}

/// The ground part of the second pass of vehicleSimulation: each collision point of the body traced from the
/// vehicle's position out to it; a point that reaches the level breaks glass there or is pushed out by a world
/// contact on the chassis.
fn sweep_chassis(map: &mut Map, bodies: &mut RigidBodies, v: &Vehicle, t: &VehicleType) -> Vec<crate::human::physics::HumanOutput> {
    let mut glass = Vec::new();
    let Some(mesh) = &t.mesh else { return glass };
    let (pos, [r0, r1, r2]) = (v.pos, v.rot);
    for p in &mesh.verts {
        let side = p.x * r0 + pos + r1 * p.y;
        let end = side + r2 * p.z;
        let m = &map.level;
        let Some(hit) = line_intersect_level(&map.ground, &m.area, &m.meshes, pos, end) else { continue };
        let hole = crate::world::capsule::CapsuleHit { pos: hit.hit.pos, normal: hit.hit.normal, dist: 0.0, area: hit.area, block: hit.block, cell: hit.cell, face_attr: hit.face_attr, set: hit.set };
        let broke = crate::human::physics::add_bullet_hole(map, hole, end, v.vel, &mut glass);
        if hit.set.is_some() {
            continue;
        }
        let n = hit.hit.normal;
        let d = hit.hit.pos - end;
        let mut depth = d.dot(n);
        let (mut at, mut normal) = (hit.hit.pos, n);
        if 0.0 > p.y {
            let raised = Vec3::new(end.x + 0.25 * 0.0, end.y + 0.25 * 1.0, end.z + 0.25 * 0.0);
            if let Some(h2) = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, raised, end) {
                let n2 = h2.hit.normal;
                let d2 = h2.hit.pos - end;
                depth = d2.dot(n2);
                at = h2.hit.pos;
                normal = n2;
            }
        }
        let (depth_scale, softness) = if broke { (1.0 / 512.0, 1.0 / 256.0) } else { (1.0 / 64.0, 0.03125) };
        if depth > MAX_CHASSIS_DEPTH {
            depth = MAX_CHASSIS_DEPTH;
        }
        let Some(cb) = bodies.get(v.body) else { continue };
        let offset = at - cb.pos;
        bodies.add_world_contact(v.body, offset, normal, depth, CHASSIS_FRICTION, depth_scale, softness);
    }
    glass
}

/// The four bottom corners of the unit cube a train's bogie probes the rails with.
const BOGIE_CORNERS: [(f32, f32); 4] = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
const BOGIE_GAUGE: f32 = 1.435;
const BOGIE_DROP: f32 = -0.5;
const BOGIE_TURN: f32 = 0.0625;
const RAIL_DEPTH_SCALE: f32 = 0.125;
const RAIL_SOFTNESS: f32 = 0.125;

/// vehicle_update_train: each bogie's angular bond turned towards the chassis about the train's up axis only, then
/// each track piece under the train holds the bogies' corners up on its rails and bed.
fn update_train(v: &mut Vehicle, bodies: &mut RigidBodies, track: &crate::world::track::Track) {
    let Some(c) = bodies.get(v.body).map(|b| b.rot[1]) else { return };
    let up = v.rot[1];
    for &(bogie, turn) in &v.bogies {
        let Some(br) = bodies.get(bogie).map(|b| b.rot[1]) else { continue };
        let target = br.cross(c) * BOGIE_TURN;
        if let Some(rosa_physics::bond::Bond::ItemAngular(a)) = bodies.bond_mut(turn) {
            a.target = target;
            a.axis_lock = Some(up);
        }
    }
    let cell = |x: f32| crate::world::trace::cvtt(x * (1.0 / 64.0));
    let (x0, x1, z0, z1) = (cell(v.bounds_min.x), cell(v.bounds_max.x), cell(v.bounds_min.z), cell(v.bounds_max.z));
    let mut candidates = Vec::new();
    for z in z0..=z1 {
        if z as u32 > 63 {
            continue;
        }
        for x in x0..=x1 {
            if x as u32 > 63 {
                continue;
            }
            candidates.extend(track.grid.get(z as usize * crate::world::track::GRID_CELLS + x as usize).into_iter().flatten().copied());
        }
    }
    let (lo, hi) = (v.bounds_min, v.bounds_max);
    for seg in candidates {
        let Some(m) = track.meshes.get(seg as usize) else { continue };
        if m.kind != 0 || m.box_min.x > hi.x || m.box_min.z > hi.z || lo.x > m.box_max.x || !(lo.z <= m.box_max.z) {
            continue;
        }
        let tris = m.rail_triangles();
        for &(bogie, _) in &v.bogies {
            for (cx, cz) in BOGIE_CORNERS {
                let Some((pos, [r0, r1, r2])) = bodies.get(bogie).map(|b| (b.pos, b.rot)) else { continue };
                let a = (cx - 0.5) * BOGIE_GAUGE;
                let d = cz - 0.5;
                let d = d + d;
                let h = BOGIE_DROP;
                let end = a * r0 + pos + d * r2 + h * r1;
                let Some((_, hp, n)) = crate::world::track::segment_intersect_triangles(&tris, pos, end) else { continue };
                let depth = ((hp.x - end.x) * n.x + (hp.y - end.y) * n.y) + n.z * (hp.z - end.z);
                bodies.add_world_contact(bogie, end - pos, n, depth, 0.0, RAIL_DEPTH_SCALE, RAIL_SOFTNESS);
                v.train_segment = seg;
            }
        }
    }
}

const TRAIN_CRUISE: f32 = 7.0 / 15.0;
const TRAIN_MOVING: f32 = -1.0 / 30.0;
const STATION_REACH: f32 = 256.0;
const STATION_PLATFORM: f32 = 16.0;
const STATION_BRAKE: f32 = 1.0 / 256.0;
const STOPPED_FAST: f32 = 1.0 / 60.0;
const STOPPED_SLOW: f32 = -1.0 / 60.0;
const STATION_WAIT: i32 = 899;
const SPEED_GAIN: f32 = 8.0;
const PULL_LIMIT: f32 = -0.875;
const BRAKE_LIMIT: f32 = 0.75;
const FULL_PULL: f32 = 0.00097222225;
const FULL_BRAKE: f32 = -1.0 / 1200.0;
const SPEEDO_SCALE: f32 = 120.0;
/// The train door is its window 3: open (broken) while the train stands, so the capsule test lets humans through.
const TRAIN_DOOR: usize = 3;
const DOOR_OPEN: i32 = 0;
const DOOR_CLOSED: i32 = 6;

/// The type 13 part of vehicleSimulation: the speedometer, the cruising speed the track's straightness allows
/// (slowing to stop at the station it is heading for, and moving on to the next after 15 seconds there), the door
/// open while it stands, and the chassis pushed along its length towards that speed.
fn drive_train(id: usize, v: &mut Vehicle, bodies: &mut RigidBodies, track: &crate::world::track::Track, out: &mut Vec<VehicleOutput>) {
    let r2 = v.rot[2];
    let s = v.vel.dot(r2);
    v.engine_rpm = (s.abs() * SPEEDO_SCALE * TICKS_PER_SECOND) as i32;
    let weight = |i: i32| track.meshes.get(i as usize).map_or(0.0, |m| m.weight);
    let mut target = if v.train_segment == -1 {
        TRAIN_CRUISE
    } else {
        let m = track.meshes.get(v.train_segment as usize);
        let (p, n) = m.map_or((0, 0), |m| (m.prev, m.next));
        let w = minss(weight(n), minss(weight(p), weight(v.train_segment)));
        w * (w * TRAIN_CRUISE)
    };
    let open = v.broken_windows.get(TRAIN_DOOR).copied().unwrap_or(false);
    if s > TRAIN_MOVING {
        if !open {
            out.push(VehicleOutput::Update { vehicle: id, kind: DOOR_OPEN, part: TRAIN_DOOR as i32, pos: v.pos, vel: v.vel });
            set_door(v, true);
        }
    } else if open {
        out.push(VehicleOutput::Update { vehicle: id, kind: DOOR_CLOSED, part: TRAIN_DOOR as i32, pos: v.pos, vel: v.vel });
        set_door(v, false);
    }
    let station = track.spawns.get(v.train_index as usize).map_or(Vec3::ZERO, |s| s.0);
    let p = v.pos;
    let (dx, dy, dz) = (station.x - p.x, station.y - p.y, station.z - p.z);
    let d = ((dy * dy + dx * dx) + dz * dz).sqrt();
    if STATION_REACH > d {
        let rel = p - station;
        let mut proj = rel.dot(r2);
        if STATION_PLATFORM > proj {
            proj = 0.0;
        }
        if v.train_reverse == 1 {
            proj = -proj;
        }
        target = (proj * STATION_BRAKE) * TRAIN_CRUISE;
        if STOPPED_FAST > s && s > STOPPED_SLOW {
            v.train_wait += 1;
            if v.train_wait > STATION_WAIT {
                v.train_index += 1;
                if v.train_index >= track.spawns.len() as i32 {
                    v.train_index = 0;
                }
                v.train_wait = 0;
            }
        } else {
            v.train_wait = 0;
        }
    }
    let y = (target + s) * SPEED_GAIN;
    let f = if PULL_LIMIT > y {
        FULL_PULL
    } else if y > BRAKE_LIMIT {
        FULL_BRAKE
    } else {
        ((0.5 * -y) * SPEED_GAIN) / 3600.0
    };
    let Some(b) = bodies.get_mut(v.body) else { return };
    let r = b.rot[2];
    b.vel = r * f + b.vel;
}

fn set_door(v: &mut Vehicle, open: bool) {
    if let Some(d) = v.broken_windows.get_mut(TRAIN_DOOR) {
        *d = open;
    }
}

fn minss(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

/// The rotor winds up 10 RPM a step to 2000 while a driver sits in an unlocked, unwrecked helicopter, and down 10 a
/// step otherwise.
const ROTOR_RPM_MAX: i32 = 2000;
const ROTOR_RPM_STEP: i32 = 10;
const ROTOR_PI: f64 = 3.14159265359;
const ROTOR_TURN: f64 = 6.28318530718;
const BLADE_PI: f64 = std::f64::consts::PI;
/// The hull's drag along its side, up and forward axes, per speed squared.
const HULL_DRAG: [f32; 3] = [1.5, 3.0, 0.375];
/// The yaw the driver's steering puts on the rotor.
const ROTOR_YAW: f32 = 1.0 / 225.0;
/// The rotor disc is pushed at 16 points around it, a sixteenth of a turn apart, 1 from its hub.
const BLADE_POINTS: usize = 16;
const BLADE_STEP: f64 = std::f64::consts::FRAC_PI_8;
const BLADE_REACH: f64 = 1.5339233875274658;
/// The fixed bite the blades' pitch starts from.
const BLADE_BITE: f64 = 0.14726215563703127;

/// The point (`x`, 0, `z`) of a body's frame, from its centre.
fn in_frame(rot: &RotMatrix, x: f32, z: f32) -> Vec3 {
    let [r0, _, r2] = *rot;
    r0 * x + r2 * z
}

/// rigidbody_apply_impulse_at_point: the impulse straight onto the velocity, and its turn about `offset` onto the
/// angular momentum.
fn apply_impulse_at(b: &mut RigidBody, offset: Vec3, j: Vec3) {
    b.vel += j;
    let l = b.ang_momentum;
    b.ang_momentum = j.cross(offset) + l;
}

/// The main and tail rotors' turn per step at `rpm`.
fn rotor_steps(rpm: i32) -> (f64, f64) {
    let r = rpm as f32;
    let main = (((r + r) as f64 * ROTOR_PI) / 60.0) / 60.0;
    let tail = ((((r * 0.125) + (r * 0.125)) as f64 * ROTOR_PI) / 60.0) / 60.0;
    (main, tail)
}

/// vehicle_update_helicopter: the rotor's speed and angles; the angular bond holding the rotor square to the hull;
/// then, while powered, the hull's drag, the driver's yaw on the rotor and the lift of the blades at 16 points around
/// the disc, pitched by the gas (collective) and the gear stick (cyclic).
fn update_helicopter(v: &mut Vehicle, bodies: &mut RigidBodies, driven: bool) {
    let Some(rotor) = v.rotor.as_mut() else { return };
    let rpm = v.engine_rpm;
    let powered = v.health > 0 && !v.locked && driven;
    let (main, tail) = if powered {
        v.engine_rpm = if rpm > ROTOR_RPM_MAX - ROTOR_RPM_STEP { ROTOR_RPM_MAX } else { rpm + ROTOR_RPM_STEP };
        rotor_steps(v.engine_rpm)
    } else if rpm >= ROTOR_RPM_STEP {
        v.engine_rpm = rpm - ROTOR_RPM_STEP;
        rotor_steps(v.engine_rpm)
    } else {
        v.engine_rpm = 0;
        (0.0, 0.0)
    };
    let a = (rotor.angles[0] as f64 + main) as f32;
    rotor.angles[0] = a;
    rotor.angles[1] = (a as f64 - tail) as f32;
    let a = a as f64;
    if a > ROTOR_PI {
        rotor.angles[0] = (a - ROTOR_TURN) as f32;
    } else if -ROTOR_PI > a {
        rotor.angles[0] = (a + ROTOR_TURN) as f32;
    }

    let (Some(hull), Some(blade)) = (bodies.get(v.body).cloned(), bodies.get(rotor.body).cloned()) else { return };
    let q = rot_matrix_to_quaternion(&multiply_matrixes(&blade.rot, &hull.rot));
    let q = if 0.0 > q[3] { q.map(|c| -c) } else { q };
    let t = quaternion_to_rotation_vector([-q[0], -q[1], -q[2], q[3]]);
    let [b0, b1, b2] = blade.rot;
    let target = b0 * t.x + b1 * t.y + b2 * t.z;
    if let Some(Bond::ItemAngular(bond)) = bodies.bond_mut(rotor.angular_bond) {
        bond.target = target;
    }
    if !powered {
        return;
    }

    let speed = hull.vel.length();
    let dir = hull.vel.normalize_or_zero();
    if let Some(h) = bodies.get_mut(v.body) {
        for k in 0..3 {
            let c = dir.dot(hull.rot[k]) * (-(HULL_DRAG[k] * (speed * speed)) / 1000.0);
            h.vel += v.rot[k] * c;
        }
    }

    let (blade_vel, w, up) = (blade.vel, blade.ang_vel, blade.rot[1]);
    let Some(b) = bodies.get_mut(rotor.body) else { return };
    b.ang_momentum += hull.rot[1] * (ROTOR_YAW * v.steer_control);
    let collective = (((v.gas_control * 0.125) * 0.25) as f64 * BLADE_PI + BLADE_BITE) as f32 as f64;
    let mut angle = 0.0f32;
    for _ in 0..BLADE_POINTS {
        let (c, s) = ((angle as f64).cos(), (angle as f64).sin());
        angle = (BLADE_STEP + angle as f64) as f32;
        let r = in_frame(&blade.rot, -c as f32, -s as f32);
        let radial = (in_frame(&blade.rot, (-BLADE_REACH * c) as f32, (-BLADE_REACH * s) as f32)).normalize_or_zero();
        let point_vel = r.cross(w) + blade_vel;
        let pitch = ((((v.gear_y * 0.125) * 0.125) * 0.5) as f64 * BLADE_PI) * c + collective - ((v.gear_x * 0.125) * 0.125) as f64 * BLADE_PI * s;
        let lift = ((-(pitch as f32 as f64).sin() * 240.0) / 60.0) as f32;
        let air = up * lift + point_vel;
        let m = air.length();
        let push = air.normalize_or_zero().dot(up) - (radial.dot(blade_vel) * 0.0625) * 0.5;
        apply_impulse_at(b, r, up * (((0.03125 * -(((m * m) * 0.125 * 4.0) * 0.5)) * push) * 0.3125));
        let sp = point_vel.length();
        let along = point_vel.normalize_or_zero();
        let sp2 = ((sp * sp) * 0.25) * 4.0;
        apply_impulse_at(b, r, v.rot[0] * ((along.dot(v.rot[0]) * (-(0.5 * sp2) * (1.0 / 256.0))) * 0.125));
        apply_impulse_at(b, r, v.rot[2] * ((along.dot(v.rot[2]) * (-(sp2 * 0.375) * (1.0 / 256.0))) * 0.125));
    }
}

/// vehicleSimulation: crashes and drag, then for each vehicle its drivetrain, wheels and the chassis against the
/// level. `driven` holds the vehicles with someone in the driver's seat.
pub fn vehicle_simulation(vehicles: &mut Table<Vehicle>, bodies: &mut RigidBodies, map: &mut Map, types: &[VehicleType], driven: &[usize]) -> (Vec<VehicleOutput>, Vec<crate::human::physics::HumanOutput>) {
    let mut out = Vec::new();
    let mut glass = Vec::new();
    for (id, v) in vehicles.iter_mut() {
        crash_and_drag(id, v, bodies, &mut out);
    }
    for (id, v) in vehicles.iter_mut() {
        if v.controllable_state == DRIVEN {
            update_car_drivetrain(v, &mut out);
        }
        if v.controllable_state == ROTOR || v.kind == VehicleKind::Helicopter {
            update_helicopter(v, bodies, driven.contains(&id));
        }
        if v.kind == VehicleKind::Train {
            update_train(v, bodies, &map.level.area.track);
            drive_train(id, v, bodies, &map.level.area.track, &mut out);
        }
        step_wheels(map, bodies, v);
        glass.extend(sweep_chassis(map, bodies, v, &types[v.kind as usize]));
    }
    (out, glass)
}

/// vehicles_step_wheel_constraints: each wheel body held to its mount: a spring towards the mount across the
/// chassis' up axis plus damping, extra damping along it, and the wheel's spin kept about its (steered) axle; each
/// push shared between the wheel and the chassis by their masses.
pub fn step_wheel_constraints(vehicles: &mut Table<Vehicle>, bodies: &mut Table<RigidBody>) {
    for (_, v) in vehicles.iter_mut() {
        if v.kind == VehicleKind::Helicopter || v.wheels.is_empty() {
            continue;
        }
        for w in v.wheels.iter_mut() {
            w.prev_vel = w.vel;
        }
        for k in 0..v.wheels.len() {
            let w = &v.wheels[k];
            let (Some(c), Some(wb)) = (bodies.get(v.body), bodies.get(w.body)) else { continue };
            let (mc, mw) = (c.mass, wb.mass);
            let total = mc + mw;
            let share = mc / total;
            let [r0, r1, r2] = c.rot;
            let (l, h) = (w.local_pos, w.vertical_offset);
            let a = r0 * l.x + l.y * r1 + h * r1 + r2 * l.z;
            let (cp, wp) = (c.pos, wb.pos);
            let e = (cp + a - wp) * w.spring;
            let d = wp - cp;
            let (k34, nk34) = (w.damping, -w.damping);
            let (cw, cv, wv) = (c.ang_vel, c.vel, wb.vel);
            let r = nk34 * wv + (a.cross(cw) + cv) * k34;
            let (fx, fy, fz) = (e.x + r.x, e.y + r.y, e.z + r.z);
            let n = -((fy * r1.y + fx * r1.x) + fz * r1.z);
            let g = Vec3::new(r1.x * n + fx, r1.y * n + fy, fz + n * r1.z);
            let f = -(mw / total);
            let mut wimp = wb.impulse;
            wimp = share * g + wimp;
            let mut cimp = c.impulse;
            cimp = f * g + cimp;
            let mut cang = c.ang_impulse;
            cang = g.cross(d) * f + cang;
            let k38 = w.travel_damping;
            let k9 = 0.125 * k38 * 0.75;
            let k38b = k38 * 0.375;
            let s = ((r.y * k38b + e.y * k9) * r1.y + (r.x * k38b + e.x * k9) * r1.x) + (k38b * r.z + e.z * k9) * r1.z;
            let u = r1 * s;
            wimp = share * u + wimp;
            cimp = f * u + cimp;
            cang = u.cross(d) * f + cang;
            let mut axle = r0;
            if k < 2 {
                axle = rotate_vector_about_axis(axle, r1, v.wheels[k].steer);
            }
            let wr = wb.rot[0];
            let ww = wb.ang_vel;
            let dw = cw - ww;
            let p = -dw.dot(axle);
            let perp = p * axle + dw;
            let min_i = wb.min_inertia;
            let tx = ((wr.z * axle.y - wr.y * axle.z) * 0.25 + perp.x * 0.5) * min_i + 0.0;
            let tz = ((wr.y * axle.x - axle.y * wr.x) * 0.25 + perp.z * 0.5) * min_i + 0.0;
            let ty = ((wr.x * axle.z - wr.z * axle.x) * 0.25 + perp.y * 0.5) * min_i + 0.0;
            let mut wang = wb.ang_impulse;
            wang = Vec3::new(share * tx + wang.x, share * ty + wang.y, share * tz + wang.z);
            cang = Vec3::new(tx * f + cang.x, ty * f + cang.y, tz * f + cang.z);
            let body = v.body;
            let wbody = w.body;
            if let Some(wb) = bodies.get_mut(wbody) {
                wb.impulse = wimp;
                wb.ang_impulse = wang;
            }
            if let Some(c) = bodies.get_mut(body) {
                c.impulse = cimp;
                c.ang_impulse = cang;
            }
        }
    }
}

/// apply_vehicle_wheel_forces: the brakes take each wheel's spin about its axle down (the front wheels at half
/// the brake, none at a handbrake above 1), the engine speeds up by its torque curve under the throttle and slows by
/// its losses, the clutch couples it to the gearbox, and the driven wheels' mean spin is pulled towards the
/// gearbox's.
pub fn apply_wheel_forces(vehicles: &mut Table<Vehicle>, bodies: &mut Table<RigidBody>) {
    for (_, v) in vehicles.iter_mut() {
        if v.kind == VehicleKind::Helicopter || v.wheels.is_empty() {
            continue;
        }
        let [r0, r1, _] = v.rot;
        for k in 0..v.wheels.len() {
            let wbody = v.wheels[k].body;
            let Some(wb) = bodies.get(wbody) else { continue };
            let w = wb.ang_vel;
            let (axle, spin) = match k {
                0 => {
                    let a = rotate_vector_about_axis(r0, r1, v.wheels[0].steer);
                    (a, a.dot(w))
                }
                1 => {
                    let a = rotate_vector_about_axis(r0, r1, v.wheels[1].steer);
                    (a, w.dot(a))
                }
                _ => (r0, w.dot(r0)),
            };
            v.wheels[k].spin = spin;
            let (lim, neg) = if k < 2 {
                if 1.0 >= v.brake {
                    let lim = v.brake * 0.5 / TICKS_PER_SECOND;
                    (lim, -lim)
                } else {
                    (0.0, -0.0)
                }
            } else {
                let lim = v.brake / TICKS_PER_SECOND;
                (lim, -lim)
            };
            let t = if neg > spin {
                lim
            } else if spin > lim {
                neg
            } else {
                -spin
            };
            let Some(wb) = bodies.get_mut(wbody) else { continue };
            let t = t * wb.inertia.x;
            let i = wb.ang_impulse;
            wb.ang_impulse = axle * t + i;
        }
        let es = v.engine_speed;
        let x = ((60.0 * (es * 60.0)) as f64 / FULL_TURN / if v.kind == VehicleKind::Minivan { 5000.0 } else { 7000.0 }) as f32;
        let torque = if x > 1.0 {
            0.0
        } else {
            let (one_minus, c) = if 0.0 > x {
                (1.0f32, 0.0f32)
            } else {
                let y = 1.0 - x;
                let c = (((0.0 * y) * y) * y + ((1.875 * y) * y) * x) + ((y * 2.625) * x) * x;
                let c = c + x * (x * x);
                (1.0 - c, c)
            };
            if v.kind == VehicleKind::Minivan {
                ((0.75 * one_minus) * one_minus + (one_minus + one_minus) * c) + (0.25 * c) * c
            } else if v.kind == VehicleKind::Van {
                (0.625 * c) * c + (one_minus * one_minus + c * (one_minus * 2.5))
            } else {
                (0.625 * c) * c + ((one_minus + one_minus) * c + (one_minus * 0.75) * one_minus)
            }
        };
        let torque = torque * v.throttle;
        let drive = (v.engine_power / 3600.0) * torque / (v.drive_divisor * v.engine_inertia) * 0.03125 + 0.0;
        let q = es * 0.125 * 0.125 * 0.375;
        let loss = (q as f64 * q.abs() as f64) * (1.125 - v.throttle) as f64;
        v.wheel_feedback = 64.0;
        let es_new = ((drive as f64 - loss) as f32) + es;
        v.gearbox_inertia = 8.0;
        v.engine_speed = es_new;
        let ratio = v.gear_ratio;
        let pull = if ratio == 0.0 {
            0.0
        } else {
            let r = ratio / 3.5 * 0.125;
            let inv = 1.0 / (v.engine_inertia + 8.0);
            let gs = v.gearbox_speed;
            let dd = ((es_new * 0.125 - r * gs) * v.clutch) * 0.25;
            let engine_share = ((8.0 * inv) * dd) * 0.125 * v.clutch_drag;
            v.gearbox_speed = ((dd * (v.engine_inertia * inv)) * r) * 64.0 + gs;
            0.0 - engine_share
        };
        v.engine_speed = es_new + pull;
        let mut accum = 0.0f32;
        if !v.driven.is_empty() {
            let mut sum = 0.0f32;
            for (i, &d) in v.driven.iter().enumerate() {
                let Some(wb) = v.wheels.get(d).and_then(|w| bodies.get(w.body)) else { continue };
                let axle = if i < 2 { rotate_vector_about_axis(r0, r1, v.wheels[d].steer) } else { r0 };
                let w = wb.ang_vel;
                sum += (axle.x * w.x + axle.y * w.y) + axle.z * w.z;
            }
            let avg = sum * 0.5;
            for &d in &v.driven {
                let Some(wheel) = v.wheels.get(d) else { continue };
                let diff = avg - wheel.spin;
                let steered = |k: usize| if k < 2 { rotate_vector_about_axis(r0, r1, v.wheels[k].steer) } else { r0 };
                let axle = steered(d);
                let Some(wb) = bodies.get_mut(wheel.body) else { continue };
                let imp = (diff * 0.125) * 0.125 * 0.0625 * wb.inertia.x;
                let i = wb.ang_impulse;
                wb.ang_impulse = axle * imp + i;
                let inv = 1.0 / (wheel.mass + v.gearbox_inertia);
                let dg = (v.gearbox_speed * 0.125 - avg * 0.4375) * 0.25;
                accum -= ((wheel.mass * inv) * dg) * 0.125 * v.wheel_feedback;
                let imp2 = (dg * (v.gearbox_inertia * inv)) * 0.4375;
                let axle = steered(d);
                let i = wb.ang_impulse;
                wb.ang_impulse = axle * imp2 + i;
            }
        }
        v.gearbox_speed += accum;
        let es = v.engine_speed.abs();
        if es > 0.0 && TINY > es {
            v.engine_speed = 0.0;
        }
        let gs = v.gearbox_speed.abs();
        if gs > 0.0 && TINY > gs {
            v.gearbox_speed = 0.0;
        }
    }
}

/// capsule_intersect_vehicle: the capsule `start..end` against the chassis faces of a vehicle whose bounds it may
/// reach; the nearest hit (contact point, normal, axis distance).
pub fn capsule_intersect_vehicle(v: &Vehicle, t: &VehicleType, start: Vec3, end: Vec3, radius: f32) -> Option<(Vec3, Vec3, f32)> {
    if v.kind == VehicleKind::Train {
        return capsule_intersect_train(v, t, start, end, radius);
    }
    let (s, e, mn, mx) = (start.to_array(), end.to_array(), v.bounds_min.to_array(), v.bounds_max.to_array());
    if (0..3).any(|k| mn[k] > s[k] + radius && mn[k] > e[k] + radius) || (0..3).any(|k| s[k] - radius > mx[k] && e[k] - radius > mx[k]) {
        return None;
    }
    let mesh = t.mesh.as_ref()?;
    let world: Vec<Vec3> = mesh.verts.iter().map(|&p| to_world(v, p)).collect();
    let mut best: Option<(Vec3, Vec3, f32)> = None;
    let mut best_dist = 65536.0f32;
    let mut take = |h: Option<(Vec3, Vec3, f32)>| {
        if let Some(h) = h
            && !(best_dist <= h.2)
        {
            best_dist = h.2;
            best = Some(h);
        }
    };
    for f in &t.chassis_faces {
        let (a, b, c) = (world[f[0]], world[f[1]], world[f[2]]);
        take(capsule_intersect_triangle(start, end, a, b, c, radius));
        if f.len() == 4 {
            take(capsule_intersect_triangle(start, end, a, c, world[f[3]], radius));
        }
    }
    best
}

/// capsule_intersect_vehicle_type13: a train's carriage against the capsule: the faces of its render mesh, then each
/// unbroken window from both sides (its two triangles share the first one's normal, negated for the back).
fn capsule_intersect_train(v: &Vehicle, t: &VehicleType, start: Vec3, end: Vec3, radius: f32) -> Option<(Vec3, Vec3, f32)> {
    let (s, e, mn, mx) = (start.to_array(), end.to_array(), v.bounds_min.to_array(), v.bounds_max.to_array());
    if (0..3).any(|k| mn[k] > s[k] + radius && mn[k] > e[k] + radius) || (0..3).any(|k| s[k] - radius > mx[k] && e[k] - radius > mx[k]) {
        return None;
    }
    let world: Vec<Vec3> = t.render_verts.iter().map(|&p| to_world(v, p)).collect();
    let mut best: Option<(Vec3, Vec3, f32)> = None;
    let mut best_dist = 65536.0f32;
    let mut take = |h: Option<(Vec3, Vec3, f32)>| {
        if let Some(h) = h
            && best_dist > h.2
        {
            best_dist = h.2;
            best = Some(h);
        }
    };
    for f in &t.render_faces {
        let (a, b, c) = (world[f[0]], world[f[1]], world[f[2]]);
        take(capsule_intersect_triangle(start, end, a, b, c, radius));
        if f.len() == 4 {
            take(capsule_intersect_triangle(start, end, a, c, world[f[3]], radius));
        }
    }
    for (k, w) in t.windows.iter().enumerate() {
        if v.broken_windows.get(k).copied().unwrap_or(false) {
            continue;
        }
        let [a, b, c, d] = w.map(|p| to_world(v, p));
        let n = calculate_face_normal(a, b, c);
        take(capsule_intersect_triangle_facing(start, end, a, b, c, n, radius));
        take(capsule_intersect_triangle_facing(start, end, a, c, d, n, radius));
        let back = -n;
        take(capsule_intersect_triangle_facing(start, end, a, c, b, back, radius));
        take(capsule_intersect_triangle_facing(start, end, a, d, c, back, radius));
    }
    best
}

/// The vehicle's position plus `p` turned by its orientation, in the order the collision traces add them.
fn to_world(v: &Vehicle, p: Vec3) -> Vec3 {
    let (pos, [r0, r1, r2]) = (v.pos, v.rot);
    p.x * r0 + pos + p.y * r1 + p.z * r2
}

/// The part of a vehicle a trace hit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum VehiclePart {
    /// A face of the render mesh.
    Body(usize),
    /// An unbroken window.
    Window(usize),
    Wheel(usize),
}

/// Where a trace hit a vehicle.
#[derive(Clone, Copy, Debug)]
pub struct VehicleHit {
    pub fraction: f32,
    pub pos: Vec3,
    pub normal: Vec3,
    pub part: VehiclePart,
}

/// trace_ray_vehicle without the wheels: the nearest hit as (fraction, point).
pub fn trace_vehicle(v: &Vehicle, t: &VehicleType, start: Vec3, end: Vec3) -> Option<(f32, Vec3)> {
    trace_vehicle_parts(v, t, start, end, false).map(|h| (h.fraction, h.pos))
}

/// trace_ray_vehicle (trace_segment_vehicle_collision_mesh): the segment against the faces of the body's render mesh,
/// its unbroken windows and, with `wheels`, its wheels. The types built in code trace their panels instead, over
/// particles the server never creates, but have no render vertices and so bounds nothing reaches.
pub fn trace_vehicle_parts(v: &Vehicle, t: &VehicleType, start: Vec3, end: Vec3, wheels: bool) -> Option<VehicleHit> {
    let (s, e, mn, mx) = (start.to_array(), end.to_array(), v.bounds_min.to_array(), v.bounds_max.to_array());
    if (0..3).any(|k| mn[k] > s[k] && mn[k] > e[k]) || (0..3).any(|k| s[k] > mx[k] && e[k] > mx[k]) {
        return None;
    }
    let world: Vec<Vec3> = t.render_verts.iter().map(|&p| to_world(v, p)).collect();
    let mut best = VehicleHit { fraction: 1.0, pos: Vec3::ZERO, normal: Vec3::ZERO, part: VehiclePart::Wheel(0) };
    for (k, f) in t.render_faces.iter().enumerate() {
        let (a, b, c) = (world[f[0]], world[f[1]], world[f[2]]);
        let n = calculate_face_normal(a, b, c);
        if let Some((frac, p)) = segment_intersect_face(n, start, end, a, b, c)
            && best.fraction > frac
        {
            best = VehicleHit { fraction: frac, pos: p, normal: n, part: VehiclePart::Body(k) };
        }
        if f.len() == 4 {
            let d = world[f[3]];
            let n = calculate_face_normal(a, c, d);
            if let Some((frac, p)) = segment_intersect_face(n, start, end, a, c, d)
                && !(best.fraction <= frac)
            {
                best = VehicleHit { fraction: frac, pos: p, normal: n, part: VehiclePart::Body(k) };
            }
        }
    }
    for (k, w) in t.windows.iter().enumerate() {
        if v.broken_windows.get(k).copied().unwrap_or(false) {
            continue;
        }
        let [a, b, c, d] = w.map(|p| to_world(v, p));
        let n = calculate_face_normal(a, b, c);
        if let Some((frac, p)) = segment_intersect_triangle_two_sided(n, start, end, a, b, c)
            && best.fraction > frac
        {
            best = VehicleHit { fraction: frac, pos: p, normal: n, part: VehiclePart::Window(k) };
        }
        let n = calculate_face_normal(a, c, d);
        if let Some((frac, p)) = segment_intersect_triangle_two_sided(n, start, end, a, c, d)
            && !(best.fraction <= frac)
        {
            best = VehicleHit { fraction: frac, pos: p, normal: n, part: VehiclePart::Window(k) };
        }
    }
    if wheels {
        for (k, w) in v.wheels.iter().enumerate() {
            if let Some((frac, p, n)) = segment_intersect_sphere(start, end, w.world_pos, w.radius)
                && !(best.fraction <= frac)
            {
                best = VehicleHit { fraction: frac, pos: p, normal: n, part: VehiclePart::Wheel(k) };
            }
        }
    }
    (1.0 > best.fraction).then_some(best)
}

/// segment_intersect_triangle with its two-sided flag: where the segment crosses the triangle's plane from either
/// side, when that point is inside the triangle.
fn segment_intersect_triangle_two_sided(n: Vec3, start: Vec3, end: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<(f32, Vec3)> {
    let (t, p) = segment_intersect_plane_two_sided(n, start, end, a)?;
    point_in_triangle_oriented(p, n, a, b, c).then_some((t, p))
}

/// segment_intersect_vehicle: the segment against the chassis faces of a vehicle whose bounds it may cross; the
/// nearest hit as (fraction, point, normal).
pub fn segment_intersect_vehicle(v: &Vehicle, t: &VehicleType, start: Vec3, end: Vec3) -> Option<(f32, Vec3, Vec3)> {
    let (s, e, mn, mx) = (start.to_array(), end.to_array(), v.bounds_min.to_array(), v.bounds_max.to_array());
    if (0..3).any(|k| mn[k] > s[k] && mn[k] > e[k]) || (0..3).any(|k| s[k] > mx[k] && e[k] > mx[k]) {
        return None;
    }
    let mesh = t.mesh.as_ref()?;
    let world: Vec<Vec3> = mesh.verts.iter().map(|&p| to_world(v, p)).collect();
    let mut best = (1.0f32, Vec3::ZERO, Vec3::ZERO);
    let mut take = |n: Vec3, a: Vec3, b: Vec3, c: Vec3| {
        if let Some((frac, p)) = segment_intersect_face(n, start, end, a, b, c)
            && !(best.0 <= frac)
        {
            best = (frac, p, n);
        }
    };
    for f in &t.chassis_faces {
        let (a, b, c) = (world[f[0]], world[f[1]], world[f[2]]);
        take(calculate_face_normal(a, b, c), a, b, c);
        if f.len() == 4 {
            let d = world[f[3]];
            take(calculate_face_normal(a, c, d), a, c, d);
        }
    }
    (1.0 > best.0).then_some(best)
}

/// The vehicle pass at the end of vehicleSimulation: each chassis point of a vehicle traced from its centre into
/// every other vehicle whose bounds overlap its own, pushing the two apart where it went in.
pub fn vehicle_vehicle_contacts(vehicles: &Table<Vehicle>, bodies: &mut RigidBodies, types: &[VehicleType]) {
    for (i, vi) in vehicles.iter() {
        if vi.controllable_state == HIDDEN {
            continue;
        }
        let Some(ti) = types.get(vi.kind as usize) else { continue };
        let Some(mesh) = ti.mesh.as_ref() else { continue };
        for (j, vj) in vehicles.iter() {
            if i == j || vj.controllable_state == HIDDEN || vi.occupants[0] == j as i32 || vj.occupants[0] == i as i32 {
                continue;
            }
            let (a0, a1, b0, b1) = (vi.bounds_min, vi.bounds_max, vj.bounds_min, vj.bounds_max);
            if b0.x > a1.x || b0.y > a1.y || b0.z > a1.z || a0.x > b1.x || a0.y > b1.y || a0.z > b1.z {
                continue;
            }
            if vi.kind == VehicleKind::Truck && vj.kind == VehicleKind::Pickup {
                continue;
            }
            let Some(tj) = types.get(vj.kind as usize) else { continue };
            for &n in &mesh.verts {
                let end = to_world(vi, n);
                let Some((_, hit, normal)) = segment_intersect_vehicle(vj, tj, vi.pos, end) else { continue };
                let mut depth = ((hit.x - end.x) * normal.x + (hit.y - end.y) * normal.y) + (hit.z - end.z) * normal.z;
                if !(depth <= MAX_CHASSIS_DEPTH) {
                    depth = MAX_CHASSIS_DEPTH;
                }
                let (Some(pa), Some(pb)) = (bodies.get(vi.body).map(|b| b.pos), bodies.get(vj.body).map(|b| b.pos)) else { continue };
                let off_a = hit - pa;
                let off_b = hit - pb;
                bodies.add_body_contact(vi.body, vj.body, off_a, off_b, normal, depth, VEHICLE_FRICTION, VEHICLE_DEPTH_SCALE, VEHICLE_SOFTNESS);
            }
        }
    }
}
