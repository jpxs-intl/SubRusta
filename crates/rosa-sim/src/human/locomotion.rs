use glam::Vec3;
use rosa_physics::{
    Bond, RigidBodies, RotMatrix, Table,
    body::GroundContact,
    rotation::{IDENTITY, multiply_matrixes, quaternion_multiply, quaternion_normalize, quaternion_to_rot_matrix, quaternion_to_rotation_vector, rot_matrix_to_quaternion, rotate_orientation},
};

use super::{
    Human,
    bones::BONES,
    ik::{IK_END_ORIENTATION, IkParams, three_bone_ik},
};
use crate::{
    vehicle::{Vehicle, physics::trace_vehicle, types::VehicleType},
    world::{map::Map, trace::line_intersect_level},
};

const TORSO_BONES: usize = 3;
const UP: Vec3 = Vec3::Y;
const FOOT_BONES: [usize; 2] = [12, 15];

pub const FOOT_SWING: i32 = 3;
pub const FOOT_PLANTED: i32 = 1;
pub const FOOT_FREE: i32 = 0;

/// One foot's stepping state (HumanFootControlState).
#[derive(Clone, Debug, Default)]
pub struct Foot {
    pub bond: Option<usize>,
    pub cooldown: i32,
    pub mode: i32,
    pub frames: i32,
    pub swing_phase: f32,
    pub swing_start: Vec3,
    pub plant: Vec3,
    pub target: Vec3,
    pub plant_blend: f32,
    pub ground_height: f32,
    pub plant_pitch: f32,
    pub prev_plant_pitch: f32,
    pub plant_yaw: f32,
    pub predicted_ground: Vec3,
}

/// The balance and walking state of a conscious human (HumanLocomotionState).
#[derive(Clone, Debug)]
pub struct Locomotion {
    pub jump_charge: i32,
    // TODO: name once its readers are ported (zeroed while standing on the ground)
    pub unk_04: f32,
    // TODO: name the step state copied from the human when a planted foot starts a step
    pub step_a: f32,
    pub step_b: f32,
    pub step_angle: f32,
    pub spine_frames: [RotMatrix; 3],
    pub torso_center: Vec3,
    pub torso_rot: RotMatrix,
    pub torso_quat: [f32; 4],
    pub torso_vel: Vec3,
    pub rel_vel: Vec3,
    pub torso_ang_vel: Vec3,
    pub body_ang_vel: Vec3,
    pub move_target_pos: Vec3,
    pub support_vel: Vec3,
    pub support_ang_vel: Vec3,
    pub move_target_vel: Vec3,
    pub world_move_vel: Vec3,
    pub foot_torque: [Vec3; 2],
    pub terrain_speed_scale: f32,
    pub terrain_height_delta: f32,
    pub speed_blend: f32,
    pub active_foot: i32,
    pub feet: [Foot; 2],
    pub stride_balance: f32,
}

impl Default for Locomotion {
    fn default() -> Self {
        Self {
            jump_charge: 0,
            unk_04: 0.0,
            step_a: 0.0,
            step_b: 0.0,
            step_angle: 0.0,
            spine_frames: [IDENTITY; 3],
            torso_center: Vec3::ZERO,
            torso_rot: IDENTITY,
            torso_quat: [0.0; 4],
            torso_vel: Vec3::ZERO,
            rel_vel: Vec3::ZERO,
            torso_ang_vel: Vec3::ZERO,
            body_ang_vel: Vec3::ZERO,
            move_target_pos: Vec3::ZERO,
            support_vel: Vec3::ZERO,
            support_ang_vel: Vec3::ZERO,
            move_target_vel: Vec3::ZERO,
            world_move_vel: Vec3::ZERO,
            foot_torque: [Vec3::ZERO; 2],
            terrain_speed_scale: 0.0,
            terrain_height_delta: 0.0,
            speed_blend: 0.0,
            active_foot: 0,
            feet: Default::default(),
            stride_balance: 0.0,
        }
    }
}

fn weighted(h: &Human, count: usize, value: impl Fn(&super::Bone) -> Vec3) -> Vec3 {
    let (mut sum, mut total) = (Vec3::ZERO, 0.0f32);
    for b in &h.bones[..count] {
        let v = value(b);
        let m = b.mass;
        total += m;
        sum = Vec3::new(sum.x + v.x * m, sum.y + v.y * m, sum.z + v.z * m);
    }
    if total > 0.0 {
        let inv = 1.0 / total;
        sum = Vec3::new(sum.x * inv, sum.y * inv, sum.z * inv);
    }
    sum
}

fn blend_half(new: Vec3, old: Vec3) -> Vec3 {
    Vec3::new(new.x * 0.5 + old.x * 0.5, new.y * 0.5 + old.y * 0.5, new.z * 0.5 + old.z * 0.5)
}

/// human_calculate_center_of_mass: the torso's mass centre, smoothed velocities and smoothed orientation.
pub fn calculate_center_of_mass(h: &mut Human) {
    let center = weighted(h, TORSO_BONES, |b| b.pos);
    let vel = weighted(h, TORSO_BONES, |b| b.vel);
    let torso_ang = weighted(h, TORSO_BONES, |b| b.ang_vel);
    let body_ang = weighted(h, h.bones.len(), |b| b.ang_vel);
    let l = &mut h.locomotion;
    l.torso_center = center;
    l.torso_vel = blend_half(vel, l.torso_vel);
    l.rel_vel = l.torso_vel - l.support_vel;
    l.torso_ang_vel = blend_half(torso_ang, l.torso_ang_vel);
    l.body_ang_vel = body_ang;

    l.torso_rot = h.bones[0].rot;
    let mut q = rot_matrix_to_quaternion(&l.torso_rot);
    let p = l.torso_quat;
    let dot = ((p[1] * q[1] + q[0] * p[0]) + p[2] * q[2]) + q[3] * p[3];
    if 0.0 > dot {
        q = q.map(|c| -c);
    }
    let q = [q[0] * 0.5 + p[0] * 0.5, q[1] * 0.5 + p[1] * 0.5, q[2] * 0.5 + p[2] * 0.5, q[3] * 0.5 + 0.5 * p[3]];
    l.torso_quat = quaternion_normalize(q);
    l.torso_rot = quaternion_to_rot_matrix(l.torso_quat);
}

/// What a human's feet stand on: the level and the vehicles near the human.
pub struct Surface<'a> {
    pub map: &'a Map,
    pub vehicles: &'a Table<Vehicle>,
    pub vehicle_types: &'a [VehicleType],
    pub nearby: &'a [usize],
}

/// human_trace_level_and_nearby_vehicles: the nearest hit on the segment, on the level or a nearby vehicle, and the
/// vehicle when it is one.
pub fn trace_hit(surface: &Surface, start: Vec3, end: Vec3) -> Option<(Vec3, Option<usize>)> {
    let m = surface.map;
    let level = line_intersect_level(&m.ground, &m.level.area, &m.level.meshes, start, end).map(|h| (h.hit.fraction, h.hit.pos));
    let (mut best, mut vehicle) = (level.unwrap_or((1.0, Vec3::ZERO)), None);
    for &id in surface.nearby {
        let Some(v) = surface.vehicles.get(id) else { continue };
        let Some(t) = surface.vehicle_types.get(v.kind as usize) else { continue };
        if let Some(hit) = trace_vehicle(v, t, start, end)
            && !(best.0 <= hit.0)
        {
            best = hit;
            vehicle = Some(id);
        }
    }
    (!(1.0 <= best.0)).then_some((best.1, vehicle))
}

/// The point [`trace_hit`] reaches.
pub fn trace(surface: &Surface, start: Vec3, end: Vec3) -> Option<Vec3> {
    trace_hit(surface, start, end).map(|(p, _)| p)
}

fn heading(yaw: f32) -> RotMatrix {
    let mut m = IDENTITY;
    rotate_orientation(&mut m, Vec3::Y, yaw);
    m
}

fn ground_below(surface: &Surface, p: Vec3) -> Option<f32> {
    let end = Vec3::new(UP.x * -1.75 + p.x, UP.y * -1.75 + p.y, -1.75 * UP.z + p.z);
    trace(surface, p, end).map(|hit| hit.y)
}

fn clamp_step(v: f32, step: f32) -> f32 {
    let neg = -step;
    if neg > v {
        neg
    } else if v > step {
        step
    } else {
        v
    }
}

/// human_probe_ground_and_support_motion: samples the ground ahead to set the terrain speed scale and height change.
pub fn probe_ground(h: &mut Human, surface: &Surface) {
    let head = heading(h.view_yaw);
    let p = h.bones[0].pos;
    let mut base = p;
    let center_y = match ground_below(surface, p) {
        Some(y) => {
            base.y = y;
            y
        }
        None => p.y,
    };

    let mtv = h.locomotion.move_target_vel;
    let (mut x, mut y, mut z) = (mtv.x, 0.0f32, mtv.z);
    let mut len = ((x * x + 0.0) + z * z).sqrt();
    let short = (1.0 / 60.0) - len;
    if short > 0.0 {
        let k = -short;
        x += head[2].x * k;
        y = head[2].y * k + 0.0;
        z += k * head[2].z;
        len = ((x * x + y * y) + z * z).sqrt();
    }
    let dir = if len == 0.0 {
        Vec3::ZERO
    } else {
        let inv = 1.0 / len;
        let k = 0.083333336f32;
        Vec3::new(x * inv * k, y * inv * k, z * inv * k)
    };

    let (mut lo, mut hi, mut hits) = (65536.0f32, -65536.0f32, 0);
    for i in 0..4 {
        let start = Vec3::new(UP.x * 0.75 + base.x, UP.y * 0.75 + base.y, UP.z * 0.75 + base.z);
        let end = Vec3::new(UP.x * -0.75 + base.x, UP.y * -0.75 + base.y, UP.z * -0.75 + base.z);
        let mut y0 = base.y;
        if let Some((hit, vehicle)) = trace_hit(surface, start, end) {
            if i == 0 {
                let v = vehicle.and_then(|id| surface.vehicles.get(id));
                h.locomotion.support_vel = v.map_or(Vec3::ZERO, |v| v.vel);
                h.locomotion.support_ang_vel = v.map_or(Vec3::ZERO, |v| v.ang_vel);
            }
            hits += 1;
            lo = if hit.y < lo { hit.y } else { lo };
            hi = if hit.y > hi { hit.y } else { hi };
            y0 = hit.y;
        }
        base = Vec3::new(base.x + dir.x * 8.0, dir.y * 8.0 + y0, base.z + dir.z * 8.0);
    }
    if hits == 0 {
        return;
    }

    let rise = hi - center_y;
    let spread = (hi - lo).abs();
    let a = rise.abs();
    let scale = if a > 0.625 {
        0.375
    } else if spread > 0.5 {
        if a > 0.5 { 1.0 - a } else { 0.5 }
    } else if a > spread {
        1.0 - a
    } else {
        1.0 - spread
    };
    let l = &mut h.locomotion;
    l.terrain_speed_scale += clamp_step(scale - l.terrain_speed_scale, 0.03125);
    l.terrain_height_delta += clamp_step(rise - l.terrain_height_delta, 0.03125);
}

fn length(v: Vec3) -> f32 {
    ((v.x * v.x + v.y * v.y) + v.z * v.z).sqrt()
}

/// human_update_movement_target: steers the movement target (where the body wants to be) from the inputs and the current motion.
pub fn update_movement_target(h: &mut Human, surface: &Surface, ticks: u32) {
    let target_stance = if h.input_flags & 8 != 0 { 0.625 } else { 1.0 };
    let jump_charge = h.locomotion.jump_charge;
    if jump_charge == 0 && h.is_on_ground {
        h.locomotion.unk_04 = 0.0;
    }
    let step = if jump_charge > 0 { 0.125 } else { 0.03125 };
    let s = h.stance;
    h.stance = if target_stance - step > s {
        s + step
    } else if s > target_stance + step {
        s - step
    } else {
        target_stance
    };

    let head = heading(h.view_yaw);
    probe_ground(h, surface);

    let l = &mut h.locomotion;
    let rv = l.rel_vel;
    let d = l.move_target_vel - rv;
    let len = ((d.y * d.y + d.x * d.x) + d.z * d.z).sqrt();
    let k = 0.033333335f32;
    if len > k {
        let u = if len == 0.0 {
            Vec3::ZERO
        } else {
            let inv = 1.0 / len;
            Vec3::new(d.x * inv * k, d.y * inv * k, d.z * inv * k)
        };
        l.move_target_vel = Vec3::new(u.x + rv.x, u.y + rv.y, u.z + rv.z);
    }
    l.move_target_vel.y = 0.0;
    let sv = l.support_vel;
    let mtp = l.move_target_pos;
    l.move_target_pos = Vec3::new(sv.x + mtp.x, mtp.y + sv.y, mtp.z + sv.z);

    let speed_now = length(l.move_target_vel);
    let tc = l.torso_center;
    let (dx, dz) = (l.move_target_pos.x - tc.x, l.move_target_pos.z - tc.z);
    let limit = ((0.125 * speed_now) * 0.25) * 60.0 + 0.0625;
    let dist = ((dx * dx + 0.0) + dz * dz).sqrt();
    if dist > limit {
        let (ux, uz) = if dist == 0.0 { (0.0, 0.0) } else { let inv = 1.0 / dist; (dx * inv, dz * inv) };
        l.move_target_pos.x = ux * limit + tc.x;
        l.move_target_pos.z = limit * uz + tc.z;
    }

    let mut speed = match h.movement_mode {
        0 => 0.083333336f32,
        2 => 0.025,
        _ => 0.05,
    };
    if jump_charge == 0 {
        speed *= h.stance;
    }
    speed *= h.locomotion.terrain_speed_scale;
    let o = heading(h.view_yaw + h.yaw_offset);
    let w = h.walk_input;
    let nw = -w;
    let s = h.strafe_input;
    let v = Vec3::new(((o[2].x * nw) + 0.0) + s * o[0].x, ((o[2].y * nw) + 0.0) + s * o[0].y, ((nw * o[2].z) + 0.0) + s * o[0].z);
    let vl = length(v);
    let (mut x, mut y, mut z) = if vl == 0.0 {
        (0.0, 0.0, 0.0)
    } else {
        let inv = 1.0 / vl;
        (v.x * inv, v.y * inv, v.z * inv)
    };
    let proj = (x * o[0].x + y * o[0].y) + z * o[0].z;
    let kk = 0.25 * -proj;
    x += o[0].x * kk;
    y += o[0].y * kk;
    z += kk * o[0].z;
    if 0.0 > w {
        let p2 = (y * o[2].y + x * o[2].x) + z * o[2].z;
        let k2 = -p2 * 0.5;
        x += o[2].x * k2;
        y += o[2].y * k2;
        z += k2 * o[2].z;
    }
    let mut v = Vec3::new(x * speed, y * speed, speed * z);
    let vl = length(v);
    if vl > 0.05 {
        let mut st = h.stamina;
        let tired = if ticks & 15 == 0 {
            if st > 0 {
                st -= 1;
                h.stamina = st;
                st <= 7
            } else {
                true
            }
        } else {
            st <= 7
        };
        if tired {
            v = if vl == 0.0 {
                Vec3::ZERO
            } else {
                let inv = 1.0 / vl;
                Vec3::new(v.x * inv * 0.05, v.y * inv * 0.05, v.z * inv * 0.05)
            };
        }
    }

    let l = &mut h.locomotion;
    if l.torso_rot[1].y < 0.5 {
        v = Vec3::ZERO;
    }
    let rv = l.rel_vel;
    let a = (v.x * head[0].x + head[0].y * v.y) + head[0].z * v.z;
    let b = (head[0].x * rv.x + head[0].y * rv.y) + head[0].z * rv.z;
    let t = clamp_step(((a - b) - h.lean_forward) * 0.0625, 0.0078125);
    h.lean_forward += t;
    let a2 = (v.y * head[2].y + v.x * head[2].x) + head[2].z * v.z;
    let b2 = (head[2].x * rv.x + head[2].y * rv.y) + head[2].z * rv.z;
    let dd = a2 - b2;
    let q = if dd > 0.0 { (-(dd.abs() as f64) * 0.125) as f32 } else { -dd.abs() };
    let t = clamp_step((q - h.lean_side) * 0.0625, 0.0078125);
    h.lean_side += t;

    let m = length(l.move_target_vel);
    let s2 = 60.0 * m;
    let s2 = s2 + s2;
    let f = if s2 > 1.0 { 0.0 } else { 1.0 - s2 };
    l.speed_blend += clamp_step(f - l.speed_blend, 0.03125);
    let k = 0.96875 - l.speed_blend * 0.5;
    let mtv = l.move_target_vel;
    l.move_target_vel = Vec3::new(mtv.x * k, mtv.y * k, k * mtv.z);
    let mtv = l.move_target_vel;
    let (mut rx, mut rz) = (v.x - mtv.x, v.z - mtv.z);
    let max = if h.movement_state == 5 { 0.0 } else { 0.0044444446 };
    let rl = ((rx * rx + 0.0) + rz * rz).sqrt();
    let mut ry = 0.0f32;
    if rl > max {
        if rl == 0.0 {
            rx = 0.0;
            rz = 0.0;
        } else {
            let inv = 1.0 / rl;
            rx *= inv;
            rz *= inv;
            ry = inv * 0.0;
        }
        rx *= max;
        ry *= max;
        rz *= max;
    }
    l.move_target_vel = Vec3::new(rx + mtv.x, ry + mtv.y, rz + mtv.z);
    // TODO: holding item type 28 in either hand drags the movement target towards its handle (shopping cart)
    l.move_target_vel.y = 0.0;
    let (mtp, mtv) = (l.move_target_pos, l.move_target_vel);
    l.move_target_pos = Vec3::new(mtp.x + mtv.x, mtp.y + mtv.y, mtp.z + mtv.z);
    update_stance_height(h, surface);
}

fn plant_foot(h: &mut Human, k: usize, yaw: f32) {
    let pos = h.bones[FOOT_BONES[k]].pos;
    let f = &mut h.locomotion.feet[k];
    f.mode = FOOT_PLANTED;
    f.cooldown = 5;
    f.plant = pos;
    f.plant.y = 0.03125;
    f.plant_yaw = yaw;
}

fn bone_yaw(h: &Human, bone: usize) -> f32 {
    let r = h.bones[bone].rot[2];
    ((-r.x) as f64).atan2(r.z as f64) as f32
}

fn lift_foot(h: &mut Human, k: usize, hips: &[Vec3; 2], ankles: &[Vec3; 2]) {
    let f = &mut h.locomotion.feet[k];
    f.swing_phase = 0.0;
    f.mode = FOOT_SWING;
    f.swing_start = ankles[k] - hips[k];
}

fn horizontal_len(x: f32, z: f32) -> f32 {
    ((x * x + 0.0) + z * z).sqrt()
}

/// human_update_foot_plant_states: plants, lifts and drags the feet's step points.
pub fn update_foot_plant_states(h: &mut Human, surface: &Surface, hips: &[Vec3; 2], ankles: &[Vec3; 2]) {
    for f in &mut h.locomotion.feet {
        if f.cooldown > 0 {
            f.cooldown -= 1;
        }
    }

    if h.locomotion.feet[0].mode == FOOT_PLANTED && h.locomotion.feet[1].mode == FOOT_PLANTED {
        let l = &h.locomotion;
        let (rv, tc, mtv) = (l.rel_vel, l.torso_center, l.move_target_vel);
        let (r0, r2) = (h.bones[0].rot[0], h.bones[0].rot[2]);
        let mut dist = [0.0f32; 2];
        for (k, d) in dist.iter_mut().enumerate() {
            let px = mtv.x * 16.0 + (rv.x * 8.0 + tc.x);
            let pz = (rv.z * 8.0 + tc.z) + mtv.z * 16.0;
            let side = if k == 0 { -0.09375 } else { 0.09375 };
            let qx = r2.x * -0.03125 + (r0.x * side + px);
            let qz = (r0.z * side + pz) + r2.z * -0.03125;
            let f = h.bones[FOOT_BONES[k]].pos;
            *d = horizontal_len(f.x - qx, f.z - qz);
        }
        let (f0, f1) = (h.bones[FOOT_BONES[0]].pos, h.bones[FOOT_BONES[1]].pos);
        let mid = horizontal_len((f0.x + f1.x) * 0.5 - tc.x, (f0.z + f1.z) * 0.5 - tc.z);
        if dist[0] > 0.125 || dist[1] > 0.125 || mid > 0.125 {
            let a = (dist[1] > dist[0]) as usize;
            let feet = &h.locomotion.feet;
            if feet[a].cooldown == 0 || feet[a ^ 1].cooldown != 0 {
                lift_foot(h, a, hips, ankles);
            }
        }
    }

    for k in 0..2 {
        if h.locomotion.feet[k].mode != FOOT_PLANTED {
            continue;
        }
        let pos = h.bones[FOOT_BONES[k]].pos;
        let contact = h.bones[FOOT_BONES[k]].ground_contact != 0;
        let f = &mut h.locomotion.feet[k];
        if !contact {
            f.plant_blend = 0.0;
        }
        let (dx, dz) = (pos.x - f.plant.x, pos.z - f.plant.z);
        let len = horizontal_len(dx, dz);
        if len > 0.03125 {
            let step = if len == 0.0 {
                Vec3::ZERO
            } else {
                let inv = 1.0 / len;
                Vec3::new(dx * inv * 0.03125, 0.0 * inv * 0.03125, dz * inv * 0.03125)
            };
            f.plant = Vec3::new(step.x + f.plant.x, step.y + f.plant.y, step.z + f.plant.z);
        }
    }

    for k in 0..2 {
        let f = &h.locomotion.feet[k];
        if f.mode != FOOT_SWING || !(f.swing_phase >= 1.0) {
            continue;
        }
        h.locomotion.feet[k].swing_phase = 1.0;
        let pos = h.bones[FOOT_BONES[k]].pos;
        let mut start = Vec3::new(0.375 * UP.x + pos.x, 0.375 * UP.y + pos.y, 0.375 * UP.z + pos.z);
        let mut end = Vec3::new(-0.75 * UP.x + start.x, -0.75 * UP.y + start.y, -0.75 * UP.z + start.z);
        let pelvis_y = h.bones[0].pos.y;
        if start.y > pelvis_y {
            start.y = pelvis_y;
            end = Vec3::new(start.x + UP.x * -0.375, pelvis_y + UP.y * -0.375, start.z + -0.375 * UP.z);
        }
        if trace(surface, start, end).is_some() {
            let yaw = h.view_yaw;
            h.locomotion.feet[k].plant_blend = 0.0;
            plant_foot(h, k, yaw);
        }
    }

    for k in 0..2 {
        if h.locomotion.feet[k].mode == FOOT_FREE && (1.0 / 30.0) > h.locomotion.rel_vel.y && h.bones[FOOT_BONES[k]].ground_contact != 0 {
            let yaw = bone_yaw(h, FOOT_BONES[k]);
            plant_foot(h, k, yaw);
        }
    }

    if h.movement_state == 0 && 0.5 > h.locomotion.torso_rot[1].y {
        let l = &mut h.locomotion;
        l.jump_charge = 0;
        l.feet[0].mode = FOOT_FREE;
        l.feet[1].mode = FOOT_FREE;
        if h.is_on_ground && 0.008333334 > length(l.rel_vel) {
            h.movement_state = 5;
        }
    }

    for k in 0..2 {
        let other = k ^ 1;
        if h.locomotion.feet[k].mode == FOOT_FREE && h.locomotion.feet[other].mode == FOOT_PLANTED && h.bones[FOOT_BONES[other]].ground_contact != 0 {
            let yaw = bone_yaw(h, FOOT_BONES[k]);
            plant_foot(h, k, yaw);
        }
    }

    if h.movement_state == 5 {
        h.locomotion.jump_charge = 0;
        if h.input_flags & 8 != 0 {
            let tc = h.locomotion.torso_center;
            let (f0, f1) = (h.bones[FOOT_BONES[0]].pos, h.bones[FOOT_BONES[1]].pos);
            let d0 = horizontal_len(f0.x - tc.x, f0.z - tc.z);
            let d1 = horizontal_len(f1.x - tc.x, f1.z - tc.z);
            if h.locomotion.feet[0].mode == FOOT_FREE && h.locomotion.feet[1].mode == FOOT_FREE {
                lift_foot(h, (d0 > d1) as usize, hips, ankles);
            }
        } else {
            h.locomotion.feet[0].mode = FOOT_FREE;
            h.locomotion.feet[1].mode = FOOT_FREE;
        }
        if 0.5 < h.locomotion.torso_rot[1].y || h.locomotion.torso_rot[1].y.is_nan() {
            h.getup_ticks += 1;
            if h.getup_ticks > 9 {
                h.movement_state = 0;
            }
        } else {
            h.getup_ticks = 0;
        }
    }

    let contacts = [h.bones[FOOT_BONES[0]].ground_contact != 0, h.bones[FOOT_BONES[1]].ground_contact != 0];
    for (k, contact) in contacts.into_iter().enumerate() {
        let f = &mut h.locomotion.feet[k];
        if !contact && f.mode == FOOT_PLANTED {
            f.frames += 1;
            if f.frames > 29 {
                f.mode = FOOT_FREE;
            }
        } else {
            f.frames = 0;
        }
    }
    for (k, contact) in contacts.into_iter().enumerate() {
        let f = &mut h.locomotion.feet[k];
        if f.mode != FOOT_PLANTED {
            continue;
        }
        if 1.0 > f.plant_blend {
            f.plant_blend += 0.5;
        }
        if !contact {
            f.plant_blend = 0.0;
        }
    }
}

fn limited(v: Vec3, max: f32) -> Vec3 {
    let len = length(v);
    if len > max {
        if len == 0.0 {
            Vec3::ZERO
        } else {
            let inv = 1.0 / len;
            Vec3::new(v.x * inv * max, v.y * inv * max, max * (v.z * inv))
        }
    } else {
        v
    }
}

/// human_compute_planted_foot_corrections: the push a planted foot gives the body towards the movement target
/// (returns the linear correction at the hip and the angular correction), and the foot's balancing torque.
#[allow(clippy::too_many_arguments)]
pub fn compute_planted_foot_corrections(h: &mut Human, k: usize, weight: f32, hip: Vec3, ankle: Vec3, target: Vec3, frame: &RotMatrix) -> (Vec3, Vec3) {
    let (gain, mut lin_max) = (3.0f32, 0.125f32);
    if h.pain > 0 {
        let d = h.pain as f32 / 30.0;
        lin_max *= if d > 1.0 { 0.0 } else { 1.0 - d };
    }
    let l = &h.locomotion;
    let (tc, rv, mtv) = (l.torso_center, l.rel_vel, l.move_target_vel);
    let v = target - tc;
    let a = hip - ankle;
    let la = ((a.x * a.x + a.y * a.y) + a.z * a.z).sqrt();
    let (nx, nz) = if la == 0.0 {
        (-0.0, -0.0)
    } else {
        let inv = 1.0 / la;
        let c = -0.32666668f32;
        ((a.x * inv) * c, (inv * a.z) * c)
    };
    let d = rv - mtv;
    let s9 = 0.5f32;
    let mut q = Vec3::new((nx + v.x) * 0.375 - d.x * s9, 0.375 * v.y - d.y * s9, (nz + v.z) * 0.375 - d.z * s9);
    rosa_physics::rotation::clamp_horizontal_vertical(&mut q, 0.25, 0.125);
    let mut e = q - rv;
    rosa_physics::rotation::clamp_horizontal_vertical(&mut e, lin_max, 0.125);
    if 0.0 > e.y {
        e.y = 0.0;
    }
    let (ex, mut ey, ez) = (e.x * gain, e.y * gain, e.z * gain);

    let (mut ang_max, mut rot_max) = (0.09375f32, 0.25f32);
    if h.pain > 0 {
        let d = h.pain as f32 / 30.0;
        if d > 1.0 {
            ang_max *= 0.0;
            rot_max = 0.0;
        } else {
            let r = 1.0 - d;
            rot_max = r * 0.25;
            ang_max *= r;
        }
    }
    let qa = rot_matrix_to_quaternion(frame);
    let mut qb = rot_matrix_to_quaternion(&h.locomotion.torso_rot);
    let dot = ((qa[0] * qb[0] + qa[1] * qb[1]) + qa[2] * qb[2]) + qa[3] * qb[3];
    if 0.0 > dot {
        qb = qb.map(|c| -c);
    }
    qb = [-qb[0], -qb[1], -qb[2], qb[3]];
    let r = quaternion_to_rotation_vector(quaternion_multiply(qb, qa));
    let r = limited(r, rot_max);
    let w = h.locomotion.torso_ang_vel;
    let p = Vec3::new(w.x * -2.0 + r.x, w.y * -2.0 + r.y, w.z * -2.0 + r.z);
    let f = limited(p - w, ang_max);

    let (ex, ez) = (ex * weight, ez * weight);
    if h.bones[FOOT_BONES[k]].ground_contact == 0 {
        ey *= weight * 0.75 + 0.25;
    }
    let ang = Vec3::new((f.x * gain) * weight, (f.y * gain) * weight, (gain * f.z) * weight);
    let r1 = hip - tc;
    let lin = Vec3::new(
        ((hip.x + mtv.x) + ex) + (ang.z * r1.y - ang.y * r1.z),
        ((hip.y + mtv.y) + ey) + (r1.z * ang.x - ang.z * r1.x),
        ((hip.z + mtv.z) + ez) + (ang.y * r1.x - ang.x * r1.y),
    );
    let c = Vec3::new(ey * r1.z - ez * r1.y, ez * r1.x - r1.z * ex, ex * r1.y - ey * r1.x);
    let [t0, t1, t2] = h.locomotion.torso_rot;
    let s0 = -((t0.y * c.y + t0.x * c.x) + t0.z * c.z);
    let mut g = Vec3::new(t0.x * s0 + 0.0, t0.y * s0 + 0.0, s0 * t0.z + 0.0);
    let s1 = ((t1.y * c.y + t1.x * c.x) + t1.z * c.z) * -0.25;
    g = Vec3::new(t1.x * s1 + g.x, g.y + t1.y * s1, s1 * t1.z + g.z);
    let s2 = -2.0 * ((c.y * t2.y + c.x * t2.x) + c.z * t2.z);
    g = Vec3::new(g.x + t2.x * s2, g.y + t2.y * s2, s2 * t2.z + g.z);
    h.locomotion.foot_torque[k] = g;
    (lin, ang)
}

/// human_update_swing_foot_target: moves a lifted foot's target along a curve from where it lifted to where it will land.
pub fn update_swing_foot_target(h: &mut Human, surface: &Surface, k: usize, hip: Vec3, head: &RotMatrix) {
    let l = &h.locomotion;
    let (tc, rv, mtv) = (l.torso_center, l.rel_vel, l.move_target_vel);
    let c = if k == 0 { -0.0234375f32 } else { 0.0234375 };
    if l.feet[k].swing_phase == 0.0 {
        let mut p = Vec3::new(tc.x + rv.x * 15.0, rv.y * 15.0 + tc.y, tc.z + 15.0 * rv.z);
        p = Vec3::new(p.x + head[0].x * c, p.y + head[0].y * c, p.z + head[0].z * c);
        let start = Vec3::new(UP.x * 0.25 + p.x, UP.y * 0.25 + p.y, UP.z * 0.25 + p.z);
        let end = Vec3::new(UP.x * -1.75 + p.x, UP.y * -1.75 + p.y, UP.z * -1.75 + p.z);
        if let Some(hit) = trace(surface, start, end) {
            p.y = hit.y;
        }
        h.locomotion.feet[k].predicted_ground = p;
    }

    let l = &h.locomotion;
    let mut px = tc.x + head[0].x * c;
    let mut pz = head[0].z * c + tc.z;
    px += head[2].x * -0.03125;
    pz += -0.03125 * head[2].z;
    let (a, b) = (mtv.x * 7.5, mtv.z * 7.5);
    let la = horizontal_len(a, b);
    let (ax, ay, az) = if la > 0.1875 {
        if la == 0.0 {
            (0.0, 0.0, 0.0)
        } else {
            let inv = 1.0 / la;
            (a * inv * 0.1875, 0.0 * inv * 0.1875, b * inv * 0.1875)
        }
    } else {
        (a, 0.0, b)
    };
    pz += az;
    let (e, f) = (rv.x - mtv.x, rv.z - mtv.z);
    let mut y = hip.y + ay;
    px += ax;
    let m = horizontal_len(e, f);
    let (ex, ey, ez) = if m > 0.015625 {
        if m == 0.0 {
            (0.0, 0.0, 0.0)
        } else {
            let inv = 1.0 / m;
            ((e * inv * 0.015625) * 7.5, (0.0 * inv * 0.015625) * 7.5, 7.5 * (inv * f * 0.015625))
        }
    } else {
        (e * 7.5, 0.0, 7.5 * f)
    };
    y += ey;
    let px2 = px + ex;
    let pz2 = pz + ez;
    let t = ((1.0 - l.feet[k].swing_phase) * 60.0) * 0.1875;
    let start = Vec3::new(mtv.x * t + px2, y + mtv.y * t, t * mtv.z + pz2);
    let end = Vec3::new(start.x + UP.x * -1.75, start.y + UP.y * -1.75, start.z + -1.75 * UP.z);
    let bone = FOOT_BONES[k];
    let mut gy = match trace(surface, start, end) {
        Some(hit) => hit.y,
        None => h.bones[bone].pos.y,
    };
    let fp = h.bones[bone].pos;
    let low = if h.bones[0].pos.y < fp.y { h.bones[0].pos.y } else { fp.y };
    let start2 = Vec3::new(fp.x, 0.25 + low, fp.z);
    let end2 = Vec3::new(UP.x * -0.5 + fp.x, start2.y + UP.y * -0.5, -0.5 * UP.z + fp.z);
    if let Some(hit) = trace(surface, start2, end2)
        && hit.y > gy
    {
        gy = hit.y;
    }

    let s = (h.locomotion.feet[k].plant_pitch as f64).sin() * 0.25;
    let dx = px2 - hip.x;
    let ty = ((gy as f64) - s) as f32;
    let dy = ty - hip.y;
    let dz = pz2 - hip.z;
    let moving = h.movement_state != 5;
    let f = &mut h.locomotion.feet[k];
    let mut phase = 0.05 + f.swing_phase;
    f.swing_phase = phase;
    if moving {
        phase += 1.0 / 30.0;
        f.swing_phase = phase;
    }
    let ph = if 1.0 < phase { 1.0 } else { phase };
    let sp = length(mtv) * 60.0;
    let g = (sp - 1.0) * 0.125;
    let (aa, bb, cc) = if 0.0 > g {
        (0.0, 0.0, 0.0)
    } else if g > 1.0 {
        (0.5, 0.125, 0.375)
    } else {
        (g * 0.5, 0.125 * g, 0.375 * g)
    };

    let ss = f.swing_start;
    let u = 1.0 - ph;
    let p1x = (((bb * head[2].x + ss.x) * 3.0) * u * u) * ph;
    let p1y = ((((cc + ss.y) + head[2].y * bb) * 3.0) * u * u) * ph;
    let p1z = ((((head[2].z * bb + ss.z) * 3.0) * u) * u) * ph;
    let p0 = |v: f32| ((v * u) * u) * u;
    let p2x = (((dx * 3.0) * u) * ph) * ph;
    let p2y = ((((aa + dy) * 3.0) * u) * ph) * ph;
    let p2z = ((u * (3.0 * dz)) * ph) * ph;
    let p3x = ((dx * ph) * ph) * ph;
    let p3y = ((dy * ph) * ph) * ph;
    let p3z = ph * ((dz * ph) * ph);
    let tx = ((p1x + p0(ss.x)) + p2x) + p3x;
    let ty = (p2y + (p1y + p0(ss.y))) + p3y;
    let tz = p3z + (p2z + (p1z + p0(ss.z)));
    let wmv = h.locomotion.world_move_vel;
    let f = &mut h.locomotion.feet[k];
    f.target = Vec3::new((tx + hip.x) + wmv.x, (ty + hip.y) + wmv.y, (tz + hip.z) + wmv.z);
    f.plant_blend = 0.0;
}

fn joint_point(bone: &super::Bone, row: usize, k: f32) -> Vec3 {
    let r = bone.rot[row];
    Vec3::new(k * r.x + bone.pos.x, r.y * k + bone.pos.y, r.z * k + bone.pos.z)
}

/// human_update_foot_ground_constraint: holds a foot on the ground below it with a ground contact bond for this tick.
pub fn update_foot_ground_constraint(h: &mut Human, bodies: &mut RigidBodies, surface: &Surface, k: usize) {
    let bone = FOOT_BONES[k];
    let s = (h.locomotion.feet[k].plant_pitch as f64).sin();
    let reach = (-s * 0.1875) as f32 + 0.03125;
    let nd = -reach;
    let p = h.bones[bone].pos;
    let start = Vec3::new(UP.x * 0.375 + p.x, UP.y * 0.375 + p.y, 0.375 * UP.z + p.z);
    let end = Vec3::new(UP.x * nd + p.x, UP.y * nd + p.y, nd * UP.z + p.z);
    let Some(hit) = trace(surface, start, end) else { return };
    if 0.0 > h.bones[bone - 1].rot[1].y {
        return;
    }
    h.locomotion.feet[k].ground_height = hit.y;
    let e = hit - end;
    let depth = (e.y * UP.y + e.x * UP.x) + e.z * UP.z;
    let depth = if depth < 0.0625 { depth } else { 0.0625 };

    let mode = h.locomotion.feet[k].mode;
    let state = h.movement_state;
    let mut c = GroundContact {
        body: h.bones[bone].body,
        depth,
        stiffness: 0.5,
        damping: 0.5,
        normal: UP,
        friction: 4.0,
        drift: Vec3::ZERO,
        ground_vel: h.locomotion.support_vel,
        spin: Vec3::ZERO,
        spin_gain: { let m = h.bones[bone].min_inertia; m + m },
        lift: 0.1875,
    };
    if state == 2 {
        c.spin_gain = 0.0;
    } else if state == 3 {
        c.lift = 1.0;
    }
    let yaw = if mode == FOOT_FREE || mode == FOOT_SWING {
        c.friction = 0.0;
        c.stiffness = 0.015625;
        c.damping = 0.015625;
        if state == 2 {
            c.friction = 0.1;
        }
        h.view_yaw
    } else {
        if state == 2 {
            c.friction = 0.1;
        }
        if mode == FOOT_PLANTED { h.locomotion.feet[k].plant_yaw } else { h.view_yaw }
    };
    let mut head = heading(yaw);
    let axis = head[0];
    rotate_orientation(&mut head, axis, -h.locomotion.feet[k].plant_pitch);
    let qh = rot_matrix_to_quaternion(&head);
    let mut qb = rot_matrix_to_quaternion(&h.bones[bone].rot);
    let dot = ((qh[0] * qb[0] + qh[1] * qb[1]) + qh[2] * qb[2]) + qh[3] * qb[3];
    if 0.0 > dot {
        qb = qb.map(|v| -v);
    }
    let qb = [-qb[0], -qb[1], -qb[2], qb[3]];
    let rv = quaternion_to_rotation_vector(quaternion_multiply(qh, qb));
    let rv = if mode == FOOT_FREE { Vec3::ZERO } else { limited(rv, 0.25) };
    let [r0, r1, r2] = h.bones[bone].rot;
    c.spin = Vec3::new(
        (r0.x * rv.x + r1.x * rv.y) + r2.x * rv.z,
        (r0.y * rv.x + r1.y * rv.y) + r2.y * rv.z,
        (rv.x * r0.z + rv.y * r1.z) + rv.z * r2.z,
    );
    h.bones[bone].ground_contact += 1;
    h.locomotion.feet[k].bond = bodies.create_bond(Bond::GroundContact(c));
    h.is_on_ground = true;

    if state != 3 && h.locomotion.jump_charge > 0 && h.input_flags & 4 == 0 && mode == FOOT_PLANTED && h.locomotion.feet[k].plant_blend == 0.0 {
        start_step(h, k);
    }
}

/// The start of a jump (movement state 3) off foot `k`, shared by the foot ground constraint and human_simulation.
pub fn start_step(h: &mut Human, k: usize) {
    h.movement_state = 3;
    let l = &mut h.locomotion;
    l.feet[k].mode = FOOT_PLANTED;
    l.step_a = h.unk_194;
    l.step_b = h.unk_190;
    l.active_foot = (k ^ 1) as i32;
    l.step_angle = 2.3561945;
    l.feet[0].swing_phase = 0.0;
    l.feet[1].swing_phase = 0.0;
    let pelvis = &h.bones[0];
    for (f, (hip_bone, shin)) in [(10, 11), (13, 14)].into_iter().enumerate() {
        let hip = joint_point(pelvis, 0, BONES[hip_bone].joint.x);
        let ankle = joint_point(&h.bones[shin], 1, BONES[shin + 1].joint.y);
        l.feet[f].swing_start = ankle - hip;
    }
}

fn clamp_lean(a: f32) -> f32 {
    let d = a as f64;
    if -0.7853981633975 > d {
        -0.7853982
    } else if d > 0.7853981633975 {
        0.7853982
    } else {
        a
    }
}

fn foot_heading(yaw: f32, pitch: f32) -> RotMatrix {
    let mut m = heading(yaw);
    let axis = m[0];
    rotate_orientation(&mut m, axis, -pitch);
    m
}

/// human_update_locomotion_constraints: the standing and walking balance of a conscious human. Steers the movement
/// target, steps the feet and drives the leg joints (through the leg IK) to keep the body over its feet.
pub fn update_locomotion_constraints(h: &mut Human, bodies: &mut RigidBodies, surface: &Surface, ticks: u32) {
    let tv = h.locomotion.torso_vel;
    let speed = ((tv.y * tv.y + tv.x * tv.x) + tv.z * tv.z).sqrt() * 60.0 - 0.5;
    let stride = speed.clamp(0.0, 1.0);
    let mut leg_hp = [1.0f32; 2];
    if h.left_leg_hp <= 99 {
        leg_hp[0] = h.left_leg_hp as f32 / 100.0;
    }
    if h.right_leg_hp <= 99 {
        leg_hp[1] = h.right_leg_hp as f32 / 100.0;
    }

    let upright_yaw = |h: &Human| {
        let r = h.bones[0].rot[2];
        ((-r.x) as f64).atan2(r.z as f64) as f32
    };
    if h.movement_state == 0 {
        h.is_standing = true;
        let l = &h.locomotion;
        if l.feet[0].mode == FOOT_FREE && l.feet[1].mode == FOOT_FREE {
            if h.is_on_ground && !(l.rel_vel.y > 1.0 / 60.0) {
                h.is_standing = false;
            } else {
                if h.bones[0].rot[1].y > 0.875 {
                    h.view_yaw = upright_yaw(h);
                }
                h.movement_state = 1;
                h.locomotion.jump_charge = 0;
                h.is_standing = false;
            }
        }
    } else if h.movement_state == 1 {
        h.is_standing = false;
        h.locomotion.jump_charge = 0;
        if h.bones[0].rot[1].y > 0.875 {
            h.view_yaw = upright_yaw(h);
        }
        if h.is_on_ground && 1.0 / 60.0 > h.locomotion.rel_vel.y {
            h.movement_state = 0;
            h.is_standing = true;
        }
    }

    h.locomotion.foot_torque = [Vec3::ZERO; 2];
    let ankles = [joint_point(&h.bones[11], 1, BONES[12].joint.y), joint_point(&h.bones[14], 1, BONES[15].joint.y)];
    let pelvis = &h.bones[0];
    let (p, [r0, r1, _]) = (pelvis.pos, pelvis.rot);
    let hip = |j: Vec3| Vec3::new((j.x * r0.x + p.x) + j.y * r1.x, (j.x * r0.y + p.y) + j.y * r1.y, (j.x * r0.z + p.z) + j.y * r1.z);
    let hips = [hip(BONES[10].joint), hip(BONES[13].joint)];
    let head = heading(h.view_yaw);
    let mut lean = head;

    let z0 = 0.0 * head[2].y;
    let mut balance = 0.0f32;
    for k in 0..2 {
        let (dx, dz) = (hips[k].x - ankles[k].x, hips[k].z - ankles[k].z);
        let s = ((dx * head[2].x + z0) + dz * head[2].z) * 2.0;
        if k == 0 { balance -= s } else { balance += s }
    }
    if h.locomotion.feet[0].mode == FOOT_FREE && h.locomotion.feet[1].mode == FOOT_FREE {
        balance *= 0.5;
    }
    let sb = h.locomotion.stride_balance;
    h.locomotion.stride_balance = (balance - sb) * 0.5 + sb;

    update_movement_target(h, surface, ticks);
    let roll = clamp_lean(((0.5 * h.lean_forward) * 60.0) * 0.25);
    let axis = lean[2];
    rotate_orientation(&mut lean, axis, roll);
    let side = clamp_lean(-(60.0 * (0.5 * h.lean_side)) * 0.25);
    let axis = lean[0];
    rotate_orientation(&mut lean, axis, side);
    // TODO: the ground height probed below the pelvis here is not read afterwards
    let mtp = h.locomotion.move_target_pos;
    let l = &mut h.locomotion;
    l.move_target_vel.y = 0.0;
    l.world_move_vel = Vec3::new(l.move_target_vel.x + l.support_vel.x, l.move_target_vel.y + l.support_vel.y, l.move_target_vel.z + l.support_vel.z);
    update_foot_plant_states(h, surface, &hips, &ankles);

    let (b0, b1) = (h.locomotion.feet[0].plant_blend, h.locomotion.feet[1].plant_blend);
    let sum = b0 + b1;
    let weight = if sum > 1.0 { [b0 / sum, b1 / sum] } else { [b0, b1] };
    let mut lin = [Vec3::ZERO; 2];
    let mut ang = [Vec3::ZERO; 2];
    for k in 0..2 {
        let tv = h.locomotion.torso_vel;
        lin[k] = Vec3::new(tv.x + hips[k].x, hips[k].y + tv.y, hips[k].z + tv.z);
        if h.locomotion.feet[k].mode == FOOT_PLANTED {
            (lin[k], ang[k]) = compute_planted_foot_corrections(h, k, weight[k], hips[k], ankles[k], mtp, &lean);
        }
        if h.locomotion.feet[k].mode == FOOT_SWING {
            update_swing_foot_target(h, surface, k, hips[k], &head);
        }
    }

    let mut lean_frame = h.bones[0].rot;
    {
        let qa = rot_matrix_to_quaternion(&lean);
        let mut qb = rot_matrix_to_quaternion(&h.bones[0].rot);
        let dot = ((qa[0] * qb[0] + qa[1] * qb[1]) + qa[2] * qb[2]) + qa[3] * qb[3];
        if 0.0 > dot {
            qb = qb.map(|v| -v);
        }
        let qb = [-qb[0], -qb[1], -qb[2], qb[3]];
        let rv = quaternion_to_rotation_vector(quaternion_multiply(qb, qa));
        let up = 0.5 + h.locomotion.torso_rot[1].y;
        let k = if up > 1.0 {
            0.25
        } else if 0.125 > up {
            0.03125
        } else {
            0.25 * up
        };
        let rv = Vec3::new(rv.x * k, rv.y * k, k * rv.z);
        let len = ((rv.x * rv.x + rv.y * rv.y) + rv.z * rv.z).sqrt();
        if len > 1.5258789e-5 {
            let axis = if len == 0.0 {
                Vec3::ZERO
            } else {
                let inv = 1.0 / len;
                Vec3::new(rv.x * inv, rv.y * inv, inv * rv.z)
            };
            rotate_orientation(&mut lean_frame, axis, len);
        }
    }

    for k in 0..2 {
        let bone = FOOT_BONES[k];
        let f = &h.locomotion.feet[k];
        // TODO: this edits the foot's ground contact bond from the previous tick, which the bond solver has already
        // removed; when the slot has been reused by another bond the binary edits that one instead
        if h.bones[bone].ground_contact != 0
            && let Some(id) = f.bond
            && let Some(Bond::GroundContact(c)) = bodies.bond_mut(id)
        {
            c.drift = Vec3::ZERO;
            c.friction = 0.9;
            c.stiffness = 0.5;
            c.damping = 0.5;
            if f.mode == FOOT_FREE {
                c.friction = 0.0;
                c.spin_gain = 0.0;
            } else if f.mode == FOOT_SWING {
                c.friction = 0.0;
                c.spin_gain = 0.0;
                let q = 1.0 - stride;
                let q = q * q;
                let v = 0.5 - 0.4375 * (1.0 - q * q);
                c.stiffness = v;
                c.damping = v;
            }
        }
        let f = &h.locomotion.feet[k];
        let fh = if f.mode == FOOT_PLANTED { foot_heading(f.plant_yaw, f.plant_pitch) } else { foot_heading(h.view_yaw, f.plant_pitch) };
        let qf = rot_matrix_to_quaternion(&fh);
        let qp = rot_matrix_to_quaternion(&lean_frame);
        let mut qf2 = qf;
        let dot = ((qp[0] * qf[0] + qp[1] * qf[1]) + qp[2] * qf[2]) + qp[3] * qf[3];
        if 0.0 > dot {
            qf2 = qf2.map(|v| -v);
        }
        let qf2 = [-qf2[0], -qf2[1], -qf2[2], qf2[3]];
        let mut end_rot = quaternion_multiply(qp, qf2);
        let mode = f.mode;
        if mode & !8 == 0 {
            end_rot = [0.0, 0.0, 0.0, 1.0];
        }
        let tv = h.locomotion.torso_vel;
        let mut base = Vec3::new(tv.x + hips[k].x, hips[k].y + tv.y, hips[k].z + tv.z);
        let target = if mode == FOOT_PLANTED {
            base = lin[k];
            let fhp = foot_heading(f.plant_yaw, f.plant_pitch);
            let fb = &h.bones[bone];
            let r2 = fb.rot[2];
            let c = -0.09375f32;
            let a = Vec3::new(r2.x * c + fb.pos.x, r2.y * c + fb.pos.y, c * r2.z + fb.pos.z);
            let start = Vec3::new(0.25 * UP.x + fb.pos.x, 0.25 * UP.y + fb.pos.y, 0.25 * UP.z + fb.pos.z);
            let end = Vec3::new(UP.x * -0.5 + start.x, UP.y * -0.5 + start.y, UP.z * -0.5 + start.z);
            let mut y = a.y;
            if let Some(hit) = trace(surface, start, end) {
                y = hit.y + 0.03125;
            }
            if fb.ground_contact != 0 {
                y = f.ground_height + 0.03125;
            }
            let c2 = 0.09375 - BONES[bone].offset.z;
            Vec3::new(fhp[2].x * c2 + a.x, fhp[2].y * c2 + y, c2 * fhp[2].z + a.z)
        } else {
            ang[k] = Vec3::ZERO;
            let ankle = ankles[k];
            match mode {
                FOOT_SWING => f.target,
                8 => {
                    let r1 = lean_frame[1];
                    Vec3::new(r1.x * -0.25 + base.x, r1.y * -0.25 + base.y, -0.25 * r1.z + base.z)
                }
                FOOT_FREE => {
                    let s = -h.stance * 0.75;
                    let up_c = { let y = h.locomotion.torso_rot[1].y; if 0.0 > y { 0.0 } else { y } };
                    let r1 = h.bones[0].rot[1];
                    let mut t = Vec3::new(s * r1.x + base.x, s * r1.y + base.y, s * r1.z + base.z);
                    if h.is_on_ground {
                        t.y -= up_c * 0.25;
                    } else if 1.0 / 60.0 > h.locomotion.torso_vel.y {
                        let ry = h.bones[0].rot[1].y;
                        let ry = if 0.0 > ry { 0.0 } else { ry };
                        let rv = h.locomotion.rel_vel;
                        let (sx, sy, sz) = (rv.x * ry, 0.0 * ry, ry * rv.z);
                        if h.locomotion.active_foot == k as i32 {
                            t = Vec3::new(t.x + sx * 6.0, t.y + sy * 6.0, t.z + sz * 6.0);
                        } else {
                            t = Vec3::new((t.x + sx * -3.0) + r1.x * 0.25, (t.y + sy * -3.0) + r1.y * 0.25, (t.z + sz * -3.0) + r1.z * 0.25);
                        }
                    }
                    t
                }
                _ => ankle,
            }
        };
        let d = target - base;
        let hp = leg_hp[k];
        let clamp_max = (0.75 * hp + 0.25) * 0.2945243;
        let (mut spin_limit, flags) = if mode != FOOT_FREE {
            ([0.0; 3], if 1.0 > hp { 0x12 } else { 2 })
        } else {
            let l = if 0.875 > h.bones[0].rot[1].y { 0.0078125 } else { 0.0625 };
            ([l; 3], if 1.0 > hp { 0x13 } else { 3 })
        };
        let pose_spin = if mode != FOOT_FREE { [0.0; 3] } else { [0.25; 3] };
        if h.pain > 0 {
            let d = h.pain as f32 / 30.0;
            let v = if d > 1.0 { 0.03125 } else { 0.125 - (d * 0.125) * 0.75 };
            spin_limit = [v; 3];
        }
        let params = IkParams { length: 1.0, twist: 0.0, max_turn: 0.7853982, clamp_max, pose_spin, spin_limit, flags };
        let frame = h.bones[0].rot;
        three_bone_ik(h, bodies, 0, 10 + 3 * k, d, &frame, ang[k], &params, &mut end_rot);
    }
}

/// human_update_stance_height: eases the movement target's height towards the ground ahead plus the standing height.
pub fn update_stance_height(h: &mut Human, surface: &Surface) {
    let head = heading(h.view_yaw);
    let p = h.bones[0].pos;
    let ground = ground_below(surface, p).unwrap_or(p.y);
    let mtv = h.locomotion.move_target_vel;
    let k = -0.015625f32;
    let d = Vec3::new(mtv.x + head[2].x * k, head[2].y * k + 0.0, k * head[2].z + mtv.z);
    let s0 = -3.75f32;
    let mut base = Vec3::new(p.x + d.x * s0, d.y * s0 + ground, s0 * d.z + p.z);
    let step = 3.75f32;
    let mut samples = Vec::with_capacity(8);
    for _ in 0..8 {
        let start = Vec3::new(UP.x * 0.75 + base.x, UP.y * 0.75 + base.y, UP.z * 0.75 + base.z);
        let end = Vec3::new(UP.x * -0.75 + base.x, UP.y * -0.75 + base.y, UP.z * -0.75 + base.z);
        let y0 = match trace(surface, start, end) {
            Some(hit) => {
                samples.push(hit.y);
                hit.y
            }
            None => base.y,
        };
        base = Vec3::new(base.x + d.x * step, d.y * step + y0, d.z * step + base.z);
    }

    let tilt = ((0.5 * h.view_pitch) as f64).cos() * h.locomotion.torso_rot[1].y as f64;
    let tilt = tilt as f32;
    let lean = if 0.125 > tilt { 0.5625 } else { tilt * 0.5 + 0.5 };
    let s = h.stance;
    let mut a = 0.9375 * s;
    let mut b = if h.movement_mode != 0 { a + 0.25 } else { ((s * 0.5) + 0.5) * 0.875 + 0.25 };
    a += 0.3125;
    b *= lean;
    let hd = h.locomotion.terrain_height_delta;
    if 0.0 > hd {
        a *= 0.0;
    } else if hd > 1.0 {
        b *= 0.0;
    } else {
        a *= hd;
        b *= 1.0 - hd;
    }

    let ty = h.locomotion.torso_center.y;
    let target = if samples.is_empty() {
        ty
    } else {
        for (i, foot) in FOOT_BONES.iter().enumerate() {
            let f = &h.locomotion.feet[i];
            if f.mode == 1 && h.bones[*foot].ground_contact != 0 {
                let cap = f.ground_height + 0.125;
                for s in samples.iter_mut() {
                    if *s > cap {
                        *s = cap;
                    }
                }
            }
        }
        let mut sum = 0.0f32;
        for s in &samples {
            sum += *s;
        }
        let avg = sum / samples.len() as f32;
        avg + (b + a)
    };
    let y = h.locomotion.move_target_pos.y;
    let lo = ty - 0.125;
    let clamped = if lo > y {
        lo
    } else {
        let hi = ty + 0.125;
        if hi < y { hi } else { y }
    };
    h.locomotion.move_target_pos.y = (target - clamped) * 0.25 + clamped;
}

/// The spin a push from the body's centre puts on the torso about a hip at `r` from it (one leg's torque terms in
/// human_step_locomotion_ik).
fn step_torque(r: Vec3, e: Vec3, a: Vec3, t: &RotMatrix) -> Vec3 {
    let tx = e.y * r.z - e.z * r.y;
    let ty = e.z * r.x - e.x * r.z;
    let tz = e.x * r.y - e.y * r.x;
    let [r0, r1, r2] = *t;
    let s0 = ((r0.x * tx + r0.y * ty) + r0.z * tz) * -3.0;
    let s1 = ((r1.y * ty + r1.x * tx) + r1.z * tz) * -0.75;
    let s2 = ((ty * r2.y + tx * r2.x) + tz * r2.z) * -6.0;
    Vec3::new((r1.x * s1 + (s0 * r0.x + a.x)) + s2 * r2.x, (r1.y * s1 + (r0.y * s0 + a.y)) + r2.y * s2, (s1 * r1.z + (s0 * r0.z + a.z)) + s2 * r2.z)
}

/// human_step_locomotion_ik (movement state 3): the jump. Both legs push off with the leg IK, then every body of the
/// human is given the torso's velocity plus an upward speed set by how long jump was held, and a spin from the step
/// input.
pub fn step_locomotion_ik(h: &mut Human, bodies: &mut RigidBodies, surface: &Surface) {
    let ankles = [joint_point(&h.bones[11], 1, BONES[12].joint.y), joint_point(&h.bones[14], 1, BONES[15].joint.y)];
    let pelvis = &h.bones[0];
    let (p, [r0, r1, _]) = (pelvis.pos, pelvis.rot);
    let hip = |j: Vec3| Vec3::new((j.x * r0.x + p.x) + j.y * r1.x, (j.x * r0.y + p.y) + j.y * r1.y, (j.x * r0.z + p.z) + j.y * r1.z);
    let hips = [hip(BONES[10].joint), hip(BONES[13].joint)];
    let head = heading(h.view_yaw);
    let mut frame = heading(h.view_yaw);

    let l = &mut h.locomotion;
    l.move_target_vel.y = 0.0;
    let (mtp, mtv) = (l.move_target_pos, l.move_target_vel);
    l.move_target_pos = Vec3::new(mtp.x + mtv.x, mtp.y + mtv.y, mtp.z + mtv.z);
    l.move_target_pos.y = l.torso_center.y;
    if let Some(y) = ground_below(surface, l.torso_center) {
        l.move_target_pos.y = (h.stance * 0.875 + 0.25 * frame[1].y) + y;
    }
    let l = &mut h.locomotion;
    let (tc, tv, mtp, mtv) = (l.torso_center, l.torso_vel, l.move_target_pos, l.move_target_vel);
    let q = Vec3::new((mtp.x - tc.x) * 0.375 - (tv.x - mtv.x) * 0.75, (mtp.y - tc.y) * 0.375 - (tv.y - mtv.y) * 0.75, (mtp.z - tc.z) * 0.375 - (tv.z - mtv.z) * 0.75);
    let (ex, ez) = (q.x - tv.x, q.z - tv.z);
    let ey_raw = q.y - tv.y;
    let (ey, ey3) = if ey_raw > 0.5 {
        (0.5, 1.5)
    } else if 0.0 > ey_raw {
        (0.0, 0.0)
    } else {
        (ey_raw, ey_raw * 3.0)
    };
    let (ex3, ez3) = (ex * 3.0, ez * 3.0);
    let mut push = [
        Vec3::new((hips[0].x + tv.x) + ex3, (hips[0].y + tv.y) + ey3, (hips[0].z + tv.z) + ez3),
        Vec3::new((tv.x + hips[1].x) + ex3, (tv.y + hips[1].y) + ey3, (tv.z + hips[1].z) + ez3),
    ];
    let phase = 0.125 + l.feet[0].swing_phase;
    l.feet[0].swing_phase = if 1.0 < phase { 1.0 } else { phase };

    let qa = rot_matrix_to_quaternion(&frame);
    let mut qb = rot_matrix_to_quaternion(&l.torso_rot);
    let dot = ((qa[0] * qb[0] + qa[1] * qb[1]) + qa[2] * qb[2]) + qa[3] * qb[3];
    if 0.0 > dot {
        qb = qb.map(|c| -c);
    }
    let qb = [-qb[0], -qb[1], -qb[2], qb[3]];
    let rv = quaternion_to_rotation_vector(quaternion_multiply(qb, qa));
    let w = l.torso_ang_vel;
    let t = Vec3::new(0.375 * rv.x + -0.75 * w.x, -0.75 * w.y + rv.y * 0.375, -0.75 * w.z + rv.z * 0.375);
    let d = Vec3::new(t.x - w.x, t.y - w.y, t.z - w.z);
    let len = ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt();
    let d = if len > 0.25 {
        let n = if len == 0.0 {
            Vec3::ZERO
        } else {
            let inv = 1.0 / len;
            Vec3::new(d.x * inv, d.y * inv, d.z * inv)
        };
        Vec3::new(n.x * 0.25, n.y * 0.25, n.z * 0.25)
    } else {
        d
    };
    let w2 = Vec3::new(w.x + d.x, w.y + d.y, w.z + d.z);
    frame = h.bones[0].rot;
    let len2 = ((w2.x * w2.x + w2.y * w2.y) + w2.z * w2.z).sqrt();
    if len2 > 1.5258789e-5 {
        let axis = if len2 == 0.0 {
            Vec3::ZERO
        } else {
            let inv = 1.0 / len2;
            Vec3::new(w2.x * inv, w2.y * inv, inv * w2.z)
        };
        rotate_orientation(&mut frame, axis, len2);
    }
    let a = Vec3::new(d.x * 3.0, d.y * 3.0, d.z * 3.0);

    let e = Vec3::new(ex, ey, ez);
    let mut torque = [Vec3::ZERO; 2];
    for k in 0..2 {
        let r = hips[k] - tc;
        let b = push[k];
        push[k] = Vec3::new((r.y * a.z - r.z * a.y) + b.x, (r.z * a.x - r.x * a.z) + b.y, (r.x * a.y - r.y * a.x) + b.z);
        torque[k] = step_torque(r, e, a, &h.locomotion.torso_rot);
    }

    for k in 0..2 {
        let bone = FOOT_BONES[k];
        let rest = Vec3::new(hips[k].x + tv.x, hips[k].y + tv.y, hips[k].z + tv.z);
        let planted = h.locomotion.feet[k].mode == FOOT_PLANTED;
        let contact = h.bones[bone].ground_contact != 0;
        if planted && !contact {
            torque[k] = Vec3::ZERO;
            push[k] = rest;
        }
        let phase = h.locomotion.feet[0].swing_phase;
        if !(phase < 1.0) {
            torque[k] = Vec3::ZERO;
            push[k] = rest;
        }
        let mut m = heading(h.view_yaw);
        let axis = m[0];
        rotate_orientation(&mut m, axis, -h.locomotion.feet[k].plant_pitch);
        let m = multiply_matrixes(&m, &h.bones[0].rot);
        let mut end_rot = rot_matrix_to_quaternion(&m);
        let (ankle, hip) = (ankles[k], hips[k]);
        let (target, mut clamp, mut pose, flags) = if planted {
            let target = if contact {
                ankle - push[k]
            } else {
                torque[k] = Vec3::ZERO;
                ankle - hip
            };
            (target, 0.2945243, 0.9375, IK_END_ORIENTATION)
        } else {
            let s = h.locomotion.feet[k].swing_start;
            let pp = if 1.0 < phase { 1.0 } else { phase };
            let seg = Vec3::new(0.0 - s.x, -0.375 - s.y, 0.0 - s.z);
            torque[k] = Vec3::ZERO;
            (Vec3::new(seg.x * pp + s.x, seg.y * pp + s.y, pp * seg.z + s.z), 1.1780972, 0.75, 1)
        };
        if !(phase < 1.0) {
            clamp = 0.0;
            pose = 1.0;
        }
        let params = IkParams { length: 1.0, twist: 0.0, max_turn: 0.7853982, clamp_max: clamp, pose_spin: [pose; 3], spin_limit: [0.0625; 3], flags };
        three_bone_ik(h, bodies, 0, 10 + 3 * k, target, &frame, torque[k], &params, &mut end_rot);
    }

    h.movement_state = 1;
    let l = &mut h.locomotion;
    l.active_foot = (l.feet[0].mode == FOOT_PLANTED) as i32;
    l.feet[0].mode = FOOT_FREE;
    l.feet[1].mode = FOOT_FREE;
    let sa = l.step_a;
    l.step_a = sa.clamp(-0.5, 0.5);
    let sb = l.step_b;
    let (kb, sb) = if -0.75 > sb {
        (-0.09375, -0.75)
    } else if sb > 0.75 {
        (0.09375, 0.75)
    } else {
        (0.125 * sb, sb)
    };
    l.step_b = sb;
    let c = 0.125 * l.step_a;
    let spin = Vec3::new(head[1].x * kb + head[0].x * c, head[1].y * kb + head[0].y * c, kb * head[1].z + c * head[0].z);

    let (mut total, mut com) = (0.0f32, Vec3::ZERO);
    for bone in &h.bones {
        let m = bone.mass;
        total += m;
        com = Vec3::new(com.x + bone.pos.x * m, com.y + bone.pos.y * m, com.z + bone.pos.z * m);
    }
    if total > 0.0 {
        let inv = 1.0 / total;
        com = Vec3::new(com.x * inv, com.y * inv, com.z * inv);
    }
    let charge = h.locomotion.jump_charge;
    let speed = if charge <= 7 {
        0.033333335
    } else if charge <= 15 {
        0.05
    } else if charge > 23 {
        0.083333336
    } else {
        0.06666667
    };
    let tv = h.locomotion.torso_vel;
    let lift = speed - tv.y;
    for bone in &h.bones {
        let Some(b) = bodies.get_mut(bone.body) else { continue };
        b.vel = tv;
        b.vel.y += lift;
        let r = Vec3::new(b.pos.x - com.x, b.pos.y - com.y, b.pos.z - com.z);
        let f = spin;
        b.vel = Vec3::new((f.z * r.y - f.y * r.z) + b.vel.x, (f.x * r.z - f.z * r.x) + b.vel.y, (f.y * r.x - f.x * r.y) + b.vel.z);
        b.ang_vel = f;
        let [m0, m1, m2] = b.rot;
        let s0 = ((f.y * m0.y + f.x * m0.x) + f.z * m0.z) * b.inertia.x;
        let l0 = Vec3::new(m0.x * s0 + 0.0, m0.y * s0 + 0.0, s0 * m0.z + 0.0);
        let s1 = ((f.y * m1.y + f.x * m1.x) + f.z * m1.z) * b.inertia.y;
        let l1 = Vec3::new(l0.x + m1.x * s1, m1.y * s1 + l0.y, s1 * m1.z + l0.z);
        let s2 = ((f.y * m2.y + f.x * m2.x) + f.z * m2.z) * b.inertia.z;
        b.ang_momentum = Vec3::new(l1.x + m2.x * s2, l1.y + m2.y * s2, l1.z + s2 * m2.z);
    }
    h.locomotion.torso_vel.y = 3.0 * lift + tv.y;
    // TODO: items held in either hand get the same upward speed
    h.locomotion.jump_charge = 0;
    h.locomotion.step_angle = 1.5707964;
}

/// slide_simulation (movement state 2): the human slides on its back or side; both legs are held out ahead of the
/// pelvis with stiff, tightly limited leg joints.
pub fn slide_simulation(h: &mut Human, bodies: &mut RigidBodies) {
    let ankles = [joint_point(&h.bones[11], 1, BONES[12].joint.y), joint_point(&h.bones[14], 1, BONES[15].joint.y)];
    h.is_standing = false;
    h.locomotion.feet[0].mode = FOOT_FREE;
    h.locomotion.feet[1].mode = FOOT_FREE;
    let pelvis = &h.bones[0];
    let (p, [r0, r1, _]) = (pelvis.pos, pelvis.rot);
    let hip = |j: Vec3| Vec3::new((j.x * r0.x + p.x) + j.y * r1.x, (j.x * r0.y + p.y) + j.y * r1.y, (j.x * r0.z + p.z) + j.y * r1.z);
    let hips = [hip(BONES[10].joint), hip(BONES[13].joint)];
    calculate_center_of_mass(h);
    // TODO: when not on the ground the binary traces down from the pelvis here and ignores the result

    for k in 0..2 {
        let first = 10 + 3 * k;
        let f = &mut h.locomotion.feet[k];
        f.prev_plant_pitch = f.plant_pitch;
        f.plant_pitch = 0.0;
        let tv = h.locomotion.torso_vel;
        let hipv = Vec3::new(hips[k].x + tv.x, hips[k].y + tv.y, hips[k].z + tv.z);
        let pelvis = &h.bones[0];
        let p = pelvis.pos;
        let [r0, r1, r2] = pelvis.rot;
        let pp = Vec3::new(p.x + tv.x, p.y + tv.y, p.z + tv.z);
        let a = if k == 0 {
            let up = 0.5 * r0.y;
            let s = if 0.0 > up { -0.0 } else { -up };
            Vec3::new(r0.x * s + (-0.25 * r0.x + pp.x), (-0.25 * r0.y + pp.y) + r0.y * s, r0.z * s + (pp.z + -0.25 * r0.z))
        } else {
            let v = -r0.y * 0.5;
            let s = if 0.0 > v { 0.0 } else { v };
            Vec3::new(r0.x * s + (r0.x * 0.25 + pp.x), (r0.y * 0.25 + pp.y) + r0.y * s, (0.25 * r0.z + pp.z) + s * r0.z)
        };
        let mut t = Vec3::new(
            (-0.75 * r1.x + (a.x + -0.25 * r2.x)) + r2.x * -0.125,
            (-0.75 * r1.y + (a.y + -0.25 * r2.y)) + r2.y * -0.125,
            -0.125 * r2.z + ((a.z + -0.25 * r2.z) + -0.75 * r1.z),
        );
        if !h.is_on_ground {
            let an = Vec3::new(tv.x + ankles[k].x, ankles[k].y + tv.y, tv.z + ankles[k].z);
            t = Vec3::new((t.x - an.x) * 0.015625 + an.x, (t.y - an.y) * 0.015625 + an.y, (t.z - an.z) * 0.015625 + an.z);
        }
        let target = Vec3::new(t.x - hipv.x, t.y - hipv.y, t.z - hipv.z);
        let mut end_rot = [0.0, 0.0, 0.0, 1.0];
        let params = IkParams { length: 1.0, twist: 0.0, max_turn: 0.7853982, clamp_max: 0.018_407_77, pose_spin: [0.25; 3], spin_limit: [0.0625; 3], flags: 0x18 };
        let frame = h.bones[0].rot;
        three_bone_ik(h, bodies, 0, first, target, &frame, Vec3::ZERO, &params, &mut end_rot);
        for (parent, child) in [(0, first), (first, first + 1)] {
            let (correction, angles) = super::physics::joint_limit_correction(h, parent, child);
            h.bones[child].limit_angles = angles;
            if let Some(id) = h.bones[child].joint
                && let Some(Bond::Joint(j)) = bodies.bond_mut(id)
            {
                j.limit_correction = correction;
                let len = ((correction.x * correction.x + correction.y * correction.y) + correction.z * correction.z).sqrt();
                j.limit_active = len > 0.0;
                j.spin_limit = 0.0009765625;
            }
        }
    }
}
