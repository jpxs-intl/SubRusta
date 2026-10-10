//! A human in a vehicle seat: finding a seat, the seated pose, the driver's controls and getting out.

use glam::Vec3;
use rosa_physics::{RigidBodies, RotMatrix, rotation::rotate_orientation};
use rosa_protocol::clientbound::game::ItemKind;

use super::{
    BONE_COUNT, Human,
    bones::{BONES, BoneId},
    ik::{IkParams, three_bone_ik},
    locomotion::{FOOT_FREE, calculate_center_of_mass},
};
use crate::{
    sim::items::Touchables,
    vehicle::{HANDBRAKE, Vehicle},
    world::{map::Map, trace::line_intersect_level},
};

pub(super) const ENTER_KEY: u32 = 0x800;
const LAST_VEHICLE_TICKS: i32 = 100;
const SEAT_PARAMS: IkParams = IkParams { length: 1.0, twist: 0.0, max_turn: 45_f32.to_radians(), clamp_max: 1.0546875_f32.to_radians(), spin_limit: [1.0 / 64.0; 3], flags: 0 };

/// A vehicle's seat offset in the world, in the order human_action_simulation adds it.
fn seat_point(v: &Vehicle, s: Vec3) -> Vec3 {
    let (p, [r0, r1, r2]) = (v.pos, v.rot);
    Vec3::new(
        ((p.x + s.x * r0.x) + s.y * r1.x) + r2.x * s.z,
        ((r0.y * s.x + p.y) + r1.y * s.y) + r2.y * s.z,
        ((r0.z * s.x + p.z) + r1.z * s.y) + s.z * r2.z,
    )
}

/// The pickup action with nothing in reach (human_action_simulation): the free seat nearest a point ahead of the head,
/// within 1.375.
pub(super) fn find_seat(h: &Human, touch: &Touchables) -> Option<(usize, usize)> {
    let head = &h.bones[BoneId::Head as usize];
    let (p, r2) = (head.pos, head.rot[2]);
    let at = Vec3::new(r2.x * -(3.0 / 4.0) + p.x, r2.y * -(3.0 / 4.0) + p.y, r2.z * -(3.0 / 4.0) + p.z);
    let mut best = 11.0 / 8.0;
    let mut seat = None;
    for (vid, v) in touch.vehicles.iter() {
        for (k, &s) in v.seats.iter().enumerate() {
            let q = seat_point(v, s);
            let (dx, dy, dz) = (q.x - at.x, q.y - at.y, q.z - at.z);
            let d = (dz * dz + (dx * dx + dy * dy)).sqrt();
            if best > d && !touch.occupied.contains(&(vid, k)) {
                best = d;
                seat = Some((vid, k));
            }
        }
    }
    seat
}

/// Whether nothing the driver holds keeps them from driving: empty hands, a briefcase or a phone.
fn hands_free(h: &Human, touch: &Touchables) -> bool {
    h.inventory[..2].iter().all(|slot| {
        slot.count == 0 || touch.items.get(slot.items[0] as usize).is_some_and(|i| matches!(i.item_type, ItemKind::Briefcase | ItemKind::Phone))
    })
}

/// The driver's controls handed to the vehicle (seat 0 of a car not driven by the traffic).
fn drive(h: &Human, touch: &mut Touchables, vid: usize) {
    if h.traffic_driver || h.despawn_ticks == 0 {
        return;
    }
    let free = hands_free(h, touch);
    let Some(v) = touch.vehicles.get_mut(vid) else { return };
    let walk = h.walk_input;
    v.gas_control = if 0.0 >= walk { walk } else { 0.0 };
    if free {
        v.traffic_taken |= v.traffic_car != -1;
        v.controls = h.input_flags as i32;
        v.gear_x = h.gear_x_input;
        v.steer_control = h.strafe_input;
        v.last_driver = h.player.map_or(-1, |p| p.idx() as i32);
        v.gear_y = h.gear_y_input;
        v.gas_control = walk;
    }
}

/// Rows of `m` made orthonormal from the third, each scaled to `scale`.
fn orthonormalize(m: &mut RotMatrix, scale: f32) {
    let unit = |v: Vec3| {
        let len = ((v.y * v.y + v.x * v.x) + v.z * v.z).sqrt();
        if len == 0.0 {
            Vec3::ZERO
        } else {
            let k = scale / len;
            Vec3::new(v.x * k, v.y * k, k * v.z)
        }
    };
    let [r0, _, r2] = *m;
    let r2 = unit(r2);
    let r1 = unit(Vec3::new(r2.y * r0.z - r2.z * r0.y, r2.z * r0.x - r2.x * r0.z, r2.x * r0.y - r2.y * r0.x));
    let r0 = unit(Vec3::new(r1.y * r2.z - r1.z * r2.y, r1.z * r2.x - r1.x * r2.z, r1.x * r2.y - r1.y * r2.x));
    *m = [r0, r1, r2];
}

/// The pelvis held in the seat, sliding outwards while getting out, moving and turning with the vehicle.
fn place_pelvis(h: &Human, bodies: &mut RigidBodies, v: &Vehicle, scale: f32) {
    let ([r0, r1, r2], pos) = (v.rot, v.pos);
    let s = v.seats[h.seat];
    let mut p = Vec3::new(pos.x + s.x * r0.x, pos.y + s.x * r0.y, s.x * r0.z + pos.z);
    if h.seat_exit > 0 {
        let k = (s.x * h.seat_exit as f32) / 15.0;
        p = Vec3::new(p.x + k * r0.x, p.y + k * r0.y, p.z + k * r0.z);
    }
    let (sy, sz) = (0.25 + s.y, s.z);
    let p = Vec3::new((p.x + sy * r1.x) + sz * r2.x, (p.y + sy * r1.y) + sz * r2.y, (sy * r1.z + p.z) + sz * r2.z);
    let w = bodies.get(v.body).map_or(Vec3::ZERO, |b| b.ang_vel);
    let Some(b) = bodies.get_mut(h.bones[0].body) else { return };
    b.pos = p;
    b.vel = v.vel;
    b.rot = v.rot;
    if h.seat_exit > 0 {
        orthonormalize(&mut b.rot, scale);
    }
    b.ang_vel = w;
    let i = b.inertia;
    let [r0, r1, r2] = b.rot;
    let s0 = ((r0.y * w.y + r0.x * w.x) + r0.z * w.z) * i.x;
    let l = Vec3::new(0.0 + r0.x * s0, r0.y * s0 + 0.0, 0.0 + s0 * r0.z);
    let s1 = ((r1.y * w.y + r1.x * w.x) + r1.z * w.z) * i.y;
    let l = Vec3::new(r1.x * s1 + l.x, l.y + r1.y * s1, s1 * r1.z + l.z);
    let s2 = ((w.y * r2.y + w.x * r2.x) + w.z * r2.z) * i.z;
    b.ang_momentum = Vec3::new(l.x + r2.x * s2, l.y + r2.y * s2, s2 * r2.z + l.z);
}

/// How far the legs swing out while getting out: twice the angle whose cosine is the pelvis' height above the ground
/// (at most 1), a quarter turn when there is no ground within a unit.
fn exit_leg_angle(h: &Human, map: &Map, scale: f32) -> f32 {
    let p = h.bones[0].pos;
    let end = Vec3::new(p.x - UP.x, p.y - UP.y, p.z - UP.z);
    let Some(hit) = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, p, end) else { return 45_f32.to_radians() };
    let d = p.y - hit.hit.pos.y;
    let c = if d > scale { 1.0 } else { d as f64 };
    let a = c.acos();
    (a + a) as f32
}

const UP: Vec3 = Vec3::Y;

/// The seated pose: each bone turned from its parent's orientation and placed at its joint, moving with the parent;
/// a human holding something or getting out keeps its upper body free.
fn pose_bones(h: &Human, bodies: &mut RigidBodies, map: &Map, has_item: bool, scale: f32) {
    let exit_angle = (h.seat_exit != 0).then(|| exit_leg_angle(h, map, scale));
    for j in 1..BONE_COUNT {
        if (has_item || h.seat_exit != 0) && j <= 9 {
            continue;
        }
        let t = &BONES[j];
        let Some((pp, prot, pvel)) = bodies.get(h.bones[t.parent.index()].body).map(|b| (b.pos, b.rot, b.vel)) else { continue };
        let mut rot = prot;
        let turn = |rot: &mut RotMatrix, row: usize, angle: f32| {
            let axis = rot[row];
            rotate_orientation(rot, axis, angle);
        };
        match exit_angle {
            None if j == BoneId::ThighLeft as usize || j == BoneId::ThighRight as usize => turn(&mut rot, 0, -90.0_f32.to_radians()),
            Some(a) if j == BoneId::ThighLeft as usize => turn(&mut rot, 0, ((-a * 0.5) as f64 - (33.75_f64.to_radians())) as f32),
            Some(a) if j == BoneId::ThighRight as usize => turn(&mut rot, 0, ((-a * 0.5) as f64 + (11.25_f64.to_radians())) as f32),
            Some(a) if j == BoneId::ShinLeft as usize || j == BoneId::ShinRight as usize => turn(&mut rot, 0, a),
            _ => {}
        }
        if j == BoneId::Torso as usize {
            turn(&mut rot, 1, 0.25 * h.look_yaw);
            turn(&mut rot, 0, 0.25 * h.look_pitch);
        } else if j == BoneId::Head as usize {
            turn(&mut rot, 1, 0.75 * h.look_yaw);
            turn(&mut rot, 0, 0.75 * h.look_pitch);
        }
        if h.seat == 0 {
            if j == BoneId::ShoulderLeft as usize || j == BoneId::ShoulderRight as usize {
                turn(&mut rot, 1, -h.look_yaw * 0.25);
                turn(&mut rot, 0, -3.0 * 22.5_f32.to_radians());
            } else if j == BoneId::ForearmLeft as usize || j == BoneId::ForearmRight as usize {
                turn(&mut rot, 0, -22.5_f32.to_radians());
            }
        }
        let (jt, o) = (t.joint, t.offset);
        let [a0, a1, a2] = prot;
        let p = Vec3::new(pp.x + a0.x * jt.x, a0.y * jt.x + pp.y, a0.z * jt.x + pp.z);
        let p = Vec3::new(a1.x * jt.y + p.x, a1.y * jt.y + p.y, a1.z * jt.y + p.z);
        let p = Vec3::new(p.x + a2.x * jt.z, p.y + a2.y * jt.z, a2.z * jt.z + p.z);
        let d = Vec3::new(o.x - jt.x, o.y - jt.y, o.z - jt.z);
        let [c0, c1, c2] = rot;
        let p = Vec3::new(p.x + c0.x * d.x, p.y + c0.y * d.x, c0.z * d.x + p.z);
        let p = Vec3::new(c1.x * d.y + p.x, c1.y * d.y + p.y, p.z + c1.z * d.y);
        let p = Vec3::new(p.x + c2.x * d.z, p.y + c2.y * d.z, p.z + c2.z * d.z);
        if let Some(b) = bodies.get_mut(h.bones[j].body) {
            b.rot = rot;
            b.pos = p;
            b.vel = pvel;
        }
    }
}

/// The seated part of human_simulation: the view following the vehicle, the driver's controls, the pose, and getting
/// out once the enter key has been held long enough (or straight away when hurt).
#[allow(clippy::too_many_arguments)]
pub(super) fn simulate_seated(h: &mut Human, bodies: &mut RigidBodies, map: &Map, touch: &mut Touchables, vid: usize, has_item: bool, scale: f32) {
    // TODO: types with the +0x04 flag (none of the loaded ones) bind the pelvis to the seat with a bond and push the
    // legs out of the seat plane instead of placing them
    let Some(v) = touch.vehicles.get(vid) else { return };
    let r2 = v.rot[2];
    h.view_yaw = ((-r2.x) as f64).atan2(r2.z as f64) as f32;
    h.view_pitch = 0.0;
    h.locomotion.feet[0].mode = FOOT_FREE;
    h.locomotion.feet[1].mode = FOOT_FREE;
    if h.spawn_protection > 900 {
        h.spawn_protection = 900;
    }
    if h.seat == 0 {
        drive(h, touch, vid);
    }
    let Some(v) = touch.vehicles.get(vid) else { return };
    place_pelvis(h, bodies, v, scale);
    pose_bones(h, bodies, map, has_item, scale);
    h.last_vehicle_cooldown = LAST_VEHICLE_TICKS;
    h.last_vehicle = vid as i32;
    if h.input_flags & ENTER_KEY == 0 {
        h.seat_exit = 0;
        return;
    }
    if h.seat_exit <= 0 {
        return;
    }
    let u = h.unk_b4;
    let speed = ((u.y * u.y + u.x * u.x) + u.z * u.z).sqrt();
    h.seat_exit += 1;
    if h.seat_exit <= (scale + speed * 60.0) as i32 + 29 {
        return;
    }
    h.look_pitch = h.view_pitch;
    h.look_yaw = 0.0;
    h.seat_exit = 0;
    h.client_body_yaw = h.body_yaw;
    leave_seat(h, touch, vid);
    let Some(v) = touch.vehicles.get(vid) else { return };
    let from = v.pos;
    for j in (0..BONE_COUNT).filter(|&j| j != BoneId::FootLeft as usize && j != BoneId::FootRight as usize) {
        if let Some(hit) = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, from, h.bones[j].pos) {
            let (p, n) = (hit.hit.pos, hit.hit.normal);
            h.bones[j].pos = Vec3::new(n.x * (1.0 / 16.0) + p.x, n.y * (1.0 / 16.0) + p.y, n.z * (1.0 / 16.0) + p.z);
        }
    }
}

/// Out of the seat: the vehicle left with its handbrake on.
fn leave_seat(h: &mut Human, touch: &mut Touchables, vid: usize) {
    h.vehicle = None;
    let seat = h.seat;
    touch.occupied.retain(|&o| o != (vid, seat));
    if let Some(v) = touch.vehicles.get_mut(vid) {
        v.controls = HANDBRAKE;
    }
}

/// A seated human too hurt to stay: out at once, the view kept.
pub(super) fn fall_out(h: &mut Human, touch: &mut Touchables, vid: usize) {
    h.look_yaw = h.view_yaw;
    h.look_pitch = h.view_pitch;
    leave_seat(h, touch, vid);
}

/// walk_simulation: the legs of a seated human reaching for the floor ahead of the seat.
pub fn walk_simulation(h: &mut Human, bodies: &mut RigidBodies) {
    let pelvis = &h.bones[0];
    let (p, [r0, r1, r2]) = (pelvis.pos, pelvis.rot);
    let hip = |j: Vec3| Vec3::new((j.x * r0.x + p.x) + j.y * r1.x, (j.x * r0.y + p.y) + j.y * r1.y, (j.x * r0.z + p.z) + j.y * r1.z);
    let hips = [hip(BONES[BoneId::ThighLeft as usize].joint), hip(BONES[BoneId::ThighRight as usize].joint)];
    h.is_standing = false;
    h.locomotion.feet[0].mode = FOOT_FREE;
    h.locomotion.feet[1].mode = FOOT_FREE;
    calculate_center_of_mass(h);
    for k in 0..2 {
        let f = &mut h.locomotion.feet[k];
        f.prev_plant_pitch = f.plant_pitch;
        f.plant_pitch = 0.0;
        let pelvis = &h.bones[0];
        let (p, [r0, r1, r2]) = (pelvis.pos, pelvis.rot);
        let side = if k == 0 { -(5.0 / 16.0) } else { 5.0 / 16.0 };
        let base = Vec3::new(side * r0.x + p.x, p.y + side * r0.y, p.z + side * r0.z);
        let target = Vec3::new(
            ((-(3.0 / 4.0) * r2.x + base.x) + (-1.0 / 8.0) * r1.x) - hips[k].x,
            ((base.y + r2.y * -(3.0 / 4.0)) + r1.y * (-1.0 / 8.0)) - hips[k].y,
            ((base.z + r2.z * -(3.0 / 4.0)) + r1.z * (-1.0 / 8.0)) - hips[k].z,
        );
        let frame = h.bones[0].rot;
        let mut end_rot = [0.0, 0.0, 0.0, 1.0];
        three_bone_ik(h, bodies, 0, 10 + 3 * k, target, &frame, Vec3::ZERO, &SEAT_PARAMS, &mut end_rot);
    }
    let _ = r2;
}
