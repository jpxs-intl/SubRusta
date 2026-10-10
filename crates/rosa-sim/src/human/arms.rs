use glam::Vec3;
use crate::{
    sim::{item_types::ItemType, items::Touchables},
    world::{capsule::segment_closest_points, map::Map, trace::line_intersect_level},
};
use rosa_physics::{
    Bond, ItemAngular, ItemPoint, RigidBodies, RotMatrix,
    rotation::{IDENTITY, multiply_matrixes, quaternion_multiply, quaternion_to_axis_angle, quaternion_to_rot_matrix, rot_matrix_to_quaternion, rotate_orientation, rotate_vector_about_axis},
};

use super::{
    Human,
    bones::BONES,
    ik::{IK_LIMIT, IK_MIRROR, IkParams, three_bone_ik},
};

const SPINE_TURN: f32 = 0.3926991;
/// Both mouse buttons: with both hands empty the right arm points.
const POINT: u32 = 3;
/// Where the pointing hand reaches before the view turns it, and how much of the view's turn it follows.
const POINT_REACH: Vec3 = Vec3::new(-0.1875, 0.0, -0.65625);
const POINT_TURN: f32 = 0.75;
/// The limits on the pointing turn, compared in double precision.
const POINT_MIN_TURN_F64: f64 = -0.6872233929727672;
const POINT_MAX_TURN_F64: f64 = 0.7853981633974483;
const POINT_MIN_TURN: f32 = f32::from_bits(0xbf2f_eddf);
const POINT_MAX_TURN: f32 = 0.7853982;
const SWAY_SCALE: f32 = 0.0078125;
const ARM_LENGTH: f32 = 0.65625;
/// A seated human's empty hands reach straight ahead: the arm length turned a quarter turn, as the binary stores it.
const SEATED_REACH_Y: f64 = 2.868559968872221e-08;
const SEATED_REACH_Z: f64 = -0.6562499999999993;
const ZOOM_TILT: f32 = 0.19634954;
const GUN_PITCH_OFFSET: f64 = 0.036_815_538_909_257_82;
const GUN_CONTACT_RADIUS: f32 = 0.125;
const GUN_FRICTION: f32 = 0.4;
const GUN_DEPTH_SCALE: f32 = 0.03125;
const GUN_SOFTNESS: f32 = 0.0625;

fn hash_noise(n: i32) -> f32 {
    let h = (n << 13) ^ n;
    let m = h.wrapping_mul(h).wrapping_mul(15731).wrapping_add(789221);
    let k = m.wrapping_mul(m).wrapping_add(1376312589);
    1.0 - (k as f32) * 9.313226e-10
}

/// calculate_spread_vector: a random direction scaled to between `base` and `base + scale`, drawn from the shared
/// noise sequence `seed`.
pub fn calculate_spread_vector(seed: &mut i32, scale: f32, base: f32) -> Vec3 {
    let n = *seed;
    let v = [hash_noise(n), hash_noise(n.wrapping_add(1)), hash_noise(n.wrapping_add(2))];
    let n = n.wrapping_add(3);
    let xy = v[0] * v[0] + v[1] * v[1];
    let len = (v[2] * v[2] + xy).sqrt();
    let (x, y, z) = if len == 0.0 {
        (len, xy, 0.0)
    } else {
        let inv = 1.0 / len;
        (v[0] * inv, v[1] * inv, inv * v[2])
    };
    let r = hash_noise(n);
    *seed = n.wrapping_add(1);
    let k = base + ((r + 1.0) * scale) * 0.5;
    Vec3::new(x * k, y * k, k * z)
}

fn half_turn(angle: f32) -> (f32, f32) {
    let a = (0.5 * -angle) as f64;
    (a.sin() as f32, a.cos() as f32)
}

fn local(r: &RotMatrix, v: Vec3) -> Vec3 {
    let [r0, r1, r2] = *r;
    Vec3::new((r0.x * v.x + r0.y * v.y) + r0.z * v.z, (r1.x * v.x + r1.y * v.y) + r1.z * v.z, (r2.x * v.x + r2.y * v.y) + r2.z * v.z)
}

fn world(r: &RotMatrix, v: Vec3) -> Vec3 {
    let [r0, r1, r2] = *r;
    Vec3::new((r0.x * v.x + r1.x * v.y) + r2.x * v.z, (r0.y * v.x + r1.y * v.y) + r2.y * v.z, (r0.z * v.x + r1.z * v.y) + r2.z * v.z)
}

/// The parent's spin relative to the child, in the parent's space, as a damping term.
fn relative_spin_damping(h: &Human, parent: usize, child: usize) -> Vec3 {
    let (a, b) = (h.bones[parent].ang_vel, h.bones[child].ang_vel);
    let l = local(&h.bones[parent].rot, Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z));
    Vec3::new(l.x * -0.25, l.y * -0.25, l.z * -0.25)
}

/// The turn (axis and angle, at most `SPINE_TURN`) taking `child`, relative to `parent`, to the pose `pose`.
fn joint_turn(h: &Human, parent: usize, child: usize, pose: [f32; 4]) -> (Vec3, f32) {
    let m = multiply_matrixes(&h.bones[child].rot, &h.bones[parent].rot);
    let mut q = rot_matrix_to_quaternion(&m);
    let dot = ((pose[0] * q[0] + pose[1] * q[1]) + pose[2] * q[2]) + pose[3] * q[3];
    if 0.0 > dot {
        q = q.map(|c| -c);
    }
    let q = [-q[0], -q[1], -q[2], q[3]];
    let (axis, angle) = quaternion_to_axis_angle(quaternion_multiply(q, pose));
    (axis, angle.clamp(-SPINE_TURN, SPINE_TURN))
}

/// Turns `child` (relative to `parent`) towards the pose `pose`, with the damping term `damping`, and rebuilds the
/// predicted orientation `frame`.
fn drive_spine_joint(h: &Human, bodies: &mut RigidBodies, parent: usize, child: usize, pose: [f32; 4], damping: Vec3, frame: &mut RotMatrix) {
    let (axis, angle) = joint_turn(h, parent, child, pose);
    let t = Vec3::new(axis.x * angle * 0.75 + damping.x, axis.y * angle * 0.75 + damping.y, angle * axis.z * 0.75 + damping.z);
    let mut target = world(&h.bones[child].rot, t);
    super::ik::clamp_bone_relative_correction(h, parent, child, &mut target, 1.125, 1.0);
    let len = ((target.x * target.x + target.y * target.y) + target.z * target.z).sqrt();
    let u = if len == 0.0 {
        Vec3::ZERO
    } else {
        let inv = 1.0 / len;
        Vec3::new(target.x * inv, target.y * inv, inv * target.z)
    };
    let axis = local(&h.bones[parent].rot, u);
    *frame = multiply_matrixes(&h.bones[parent].rot, &h.bones[child].rot);
    rotate_orientation(frame, axis, len);
    if let Some(id) = h.bones[child].joint
        && let Some(Bond::Joint(j)) = bodies.bond_mut(id)
    {
        j.target_ang_vel = target;
        j.spin_limit = 0.0;
    }
}

/// Aiming a gun: `child` turns (relative to `parent`) straight to the aim pose `pose`, undamped.
fn aim_spine_joint(h: &Human, bodies: &mut RigidBodies, parent: usize, child: usize, pose: [f32; 4]) {
    let (axis, angle) = joint_turn(h, parent, child, pose);
    let target = world(&h.bones[child].rot, Vec3::new(axis.x * angle, axis.y * angle, angle * axis.z));
    if let Some(id) = h.bones[child].joint
        && let Some(Bond::Joint(j)) = bodies.bond_mut(id)
    {
        j.target_ang_vel = target;
        j.spin_limit = 0.0;
    }
}

fn pitch_turn(angle: f32) -> [f32; 4] {
    let (s, c) = half_turn(angle);
    [s, 0.0, 0.0, c]
}

fn yaw_turn(angle: f32) -> [f32; 4] {
    let (s, c) = half_turn(angle);
    [0.0, s, 0.0, c]
}

/// human_build_locomotion_orientation: the waist and chest poses of a human aiming a gun `yaw` to the side, and the
/// aim itself (stored conjugated).
fn aim_orientations(h: &Human, yaw: f32) -> [[f32; 4]; 3] {
    let identity = [0.0, 0.0, 0.0, 1.0];
    let waist = quaternion_multiply(identity, yaw_turn(0.5 * h.free_look_yaw + h.yaw_offset));
    let pitch = pitch_turn(0.5 * h.view_pitch);
    let waist = quaternion_multiply(waist, pitch);
    let aim = quaternion_multiply(identity, pitch);
    let waist = quaternion_multiply(waist, pitch_turn(0.75 * h.free_look_pitch));
    let pitch = pitch_turn(0.25 * h.view_pitch);
    let chest = quaternion_multiply(identity, pitch);
    let aim = quaternion_multiply(aim, pitch);
    let turn = yaw_turn(yaw);
    let chest = quaternion_multiply(chest, turn);
    let aim = quaternion_multiply(aim, turn);
    let chest = quaternion_multiply(chest, yaw_turn(0.25 * h.free_look_yaw));
    [waist, chest, [-aim[0], -aim[1], -aim[2], aim[3]]]
}

fn add_turn(pose: [f32; 4], frame: &mut RotMatrix, row: usize, angle: f32) -> [f32; 4] {
    let (s, c) = half_turn(angle);
    let q = if row == 0 { [s, 0.0, 0.0, c] } else { [0.0, s, 0.0, c] };
    let axis = frame[row];

    rotate_orientation(frame, axis, angle);
    quaternion_multiply(pose, q)
}

/// human_calculate_arm_angles: turns the waist, chest and head towards where the human looks and drives the arms
/// (through the arm IK) towards their pose.
pub fn calculate_arm_angles(h: &mut Human, bodies: &mut RigidBodies, map: &Map, touch: &Touchables, seed: &mut i32) {
    // TODO: throwing, two-handed and phone poses, vehicles and the aim offsets they use
    if h.action_type != 0 && h.input_flags & 0x20 != 0 {
        h.throw_pitch = 8.0 * h.free_look_pitch;
    }
    let mut arm_hp = [1.0f32; 2];
    if h.left_arm_hp <= 99 {
        arm_hp[0] = h.left_arm_hp as f32 / 100.0;
    }

    if h.right_arm_hp <= 99 {
        arm_hp[1] = h.right_arm_hp as f32 / 100.0;
    }

    if h.movement_state == 1 && 0.0 > h.locomotion.torso_vel.y {
        h.locomotion.step_angle = 0.0;
    }

    let hand_type = |slot: usize| {
        let s = &h.inventory[slot];
        (s.count > 0).then(|| touch.items.get(s.items[0] as usize)).flatten().map(|i| &touch.types[i.item_type as usize])
    };
    let has_gun = hand_type(0).is_some_and(|t| t.is_gun) || hand_type(1).is_some_and(|t| t.is_gun);
    let mut aim = if h.movement_mode == 2 { 0.7853982 } else { SPINE_TURN };
    if h.input_flags & 2 != 0 || !has_gun {
        aim = 0.0;
    }
    if hand_type(0).is_some_and(|t| t.mirrored_aim != 0) {
        aim = -aim;
    }

    let (vehicle_yaw, vehicle_pitch) = if h.vehicle.is_some() { (0.0 + h.look_yaw, 0.0 + h.look_pitch) } else { (0.0f32, 0.0f32) };
    let body_yaw = if h.vehicle.is_some() { (aim + vehicle_yaw) * 0.5 } else { (((vehicle_yaw + h.free_look_yaw * 0.5) + h.yaw_offset) + aim) * 0.5 };
    let aiming = h.input_flags & 0x20 != 0;

    let identity = [0.0, 0.0, 0.0, 1.0];
    let mut frames = [IDENTITY; 3];
    let waist_pitch = if aiming { 0.5 * h.view_pitch } else { h.view_pitch * 0.75 + 0.75 * h.free_look_pitch };
    let waist_pitch = if h.vehicle.is_some() { vehicle_pitch } else { waist_pitch } * 0.75;
    let mut frame = IDENTITY;
    let pose = add_turn(identity, &mut frame, 1, body_yaw);
    let pose = add_turn(pose, &mut frame, 0, waist_pitch);
    drive_spine_joint(h, bodies, 0, 1, pose, relative_spin_damping(h, 0, 1), &mut frames[0]);

    let chest_pitch = if aiming { 0.5 * h.view_pitch } else { 0.25 * h.view_pitch + 0.75 * h.free_look_pitch };
    let chest_pitch = if h.vehicle.is_some() { vehicle_pitch } else { chest_pitch } * 0.5;
    let mut frame = IDENTITY;
    let pose = add_turn(identity, &mut frame, 0, chest_pitch);
    let pose = add_turn(pose, &mut frame, 1, body_yaw);
    let chest_damping = relative_spin_damping(h, 1, 2);
    drive_spine_joint(h, bodies, 1, 2, pose, chest_damping, &mut frames[1]);
    let aim_poses = (has_gun && h.vehicle.is_none()).then(|| aim_orientations(h, aim));
    if let Some([waist, chest, _]) = aim_poses {
        aim_spine_joint(h, bodies, 0, 1, waist);
        aim_spine_joint(h, bodies, 1, 2, chest);
    }

    let mut frame = IDENTITY;
    let mut pose = add_turn(identity, &mut frame, 1, 0.5 * h.free_look_yaw - aim);
    if h.movement_mode == 2 && has_gun {
        pose = quaternion_multiply(pose, [0.0, 0.0, f32::from_bits(0xbdc8bd36), f32::from_bits(0x3f7ec46d)]);

        let axis = frame[2];
        rotate_orientation(&mut frame, axis, ZOOM_TILT);
    }
    let mut pose = add_turn(pose, &mut frame, 0, 0.5 * h.view_pitch + 0.25 * h.free_look_pitch);
    if let Some([_, _, aimed]) = aim_poses {
        pose = quaternion_multiply(aimed, pitch_turn(h.view_pitch));
        pose = quaternion_multiply(pose, yaw_turn(0.25 * h.free_look_yaw));
    }
    // TODO: the binary damps the head with the chest's relative spin, not the head's
    drive_spine_joint(h, bodies, 2, 3, pose, chest_damping, &mut frames[2]);
    h.locomotion.spine_frames = frames;

    let state = h.movement_state;
    for k in 0..2 {
        let first = 4 + 3 * k;
        let held = held_item(h, touch, k);
        let hold = held.map(|(item, hand)| hold_frame(h, bodies, touch, k, item, hand, -aim, aim_poses.map(|p| p[2])));
        if k == 0 {
            let s = calculate_spread_vector(seed, SWAY_SCALE, 0.0);
            let v = h.hand_sway_vel;
            let v = Vec3::new(s.x + v.x, s.y + v.y, s.z + v.z);
            let b = h.hand_sway;
            let v = Vec3::new(b.x * -0.00390625 + v.x, b.y * -0.00390625 + v.y, b.z * -0.00390625 + v.z);
            let v = Vec3::new(v.x * 0.9375, v.y * 0.9375, v.z * 0.9375);
            h.hand_sway_vel = v;
            h.hand_sway = Vec3::new(v.x + b.x, b.y + v.y, b.z + v.z);
        }
        let mut swing = (h.locomotion.stride_balance as f64 * 0.39269908169875) as f32;
        if h.movement_mode > 0 {
            swing *= 0.5;
        }
        if k == 1 {
            swing = -swing;
        }
        let (mut angle, mut scale) = match (h.movement_mode != 0, h.locomotion.jump_charge > 0) {
            (false, true) => (0.7853982, 0.75f32),
            (false, false) => ((swing as f64 + 0.39269908169875) as f32, 0.75),
            (true, true) => (0.7853982, 0.875),
            (true, false) => ((swing as f64 + 0.196349540849375) as f32, 0.875),
        };
        if state & !2 == 1 {
            angle = (1.570796326795 - (h.view_pitch + h.free_look_pitch) as f64) as f32;
            scale = 0.9375;
        }
        let a = angle as f64;
        let (y, z) = if h.vehicle.is_some() {
            ((SEATED_REACH_Y * scale as f64) as f32, (scale as f64 * SEATED_REACH_Z) as f32)
        } else {
            (((-0.65625 * a.cos()) * scale as f64) as f32, (scale as f64 * (-a.sin() * 0.65625)) as f32)
        };
        let mut target = match hold {
            Some((m, v)) => {
                let j = BONES[first].joint;
                let m1 = m[1];
                Vec3::new((0.0625 * m1.x + v.x) - j.x, (0.0625 * m1.y + v.y) - j.y, (0.0625 * m1.z + v.z) - j.z)
            }
            None => Vec3::new(if k == 0 { -0.0625 } else { 0.0625 }, y, z),
        };
        if k == 1 && h.inventory[0].count <= 0 && h.inventory[1].count <= 0 && h.input_flags & POINT == POINT {
            target = point_target(h);
        }
        if state != 2 && h.is_on_ground && 0.5 > h.locomotion.torso_rot[1].y && h.input_flags & 8 != 0 {
            target = crouch_reach(h, map, first);
        }

        let m = hold.map_or(IDENTITY, |(m, _)| m);
        let q = rot_matrix_to_quaternion(&m);
        let mut end_rot = [-q[0], -q[1], -q[2], q[3]];
        let mut max_turn = if h.action_type != 0 { 0.19634955 } else { 0.7853982 };
        if h.locomotion.jump_charge > 0 || state == 3 {
            max_turn = 0.7853982;
        }
        let hp = arm_hp[k];
        if let Some((item, hand)) = held {
            let kind = touch.items.get(item).unwrap().item_type as u32;
            let mut twist = match kind {
                0xb => 0.3926991,
                3 => 1.3744468,
                _ => 0.7853982,
            };
            if k == 0 {
                twist = -twist;
            }
            if h.input_flags & 0x20 != 0 || h.action_type == 0 {
                twist = 0.0;
            }
            let mut spin_limit = [0.0; 3];
            if 1.0 > hp {
                let v = (hp * 0.0625) * 0.25 + 0.00390625;
                spin_limit[0] = v;
                spin_limit[1] = v;
            }
            let params = IkParams { length: ARM_LENGTH, twist, max_turn, clamp_max: 0.2945243, pose_spin: [0.875; 3], spin_limit, flags: 0x16 + (1.0 > hp) as u32 };
            three_bone_ik(h, bodies, 2, first, target, &IDENTITY, Vec3::ZERO, &params, &mut end_rot);
            attach_item_to_bone(h, bodies, touch, first + 2, item, hand);
            if touch.types[kind as usize].is_gun && k == 1 && h.input_flags & 0x20 == 0 && h.action_type != 0 && h.vehicle.is_none() {
                arm_item_collision(h, bodies, touch, item);
            }
            continue;
        }
        let mut flags = IK_MIRROR + (1.0 > hp) as u32;
        let other_hand_input = 1 << (k ^ 1);
        let one_arm = h.input_flags & other_hand_input == other_hand_input;
        if one_arm {
            max_turn = SPINE_TURN;
        }
        let airborne = !one_arm && h.vehicle.is_none() && ((state == 1 && h.input_flags & 8 == 0) || (state == 3 && !(0.125 + h.locomotion.feet[0].swing_phase < 1.0)));
        let params = if airborne {
            IkParams { length: ARM_LENGTH, twist: 0.0, max_turn, clamp_max: 0.07363108, pose_spin: [0.75; 3], spin_limit: [0.0625; 3], flags }
        } else {
            flags |= IK_LIMIT;
            let twist = if k == 0 { -0.0 } else { 0.0 };
            let mut spin_limit = [0.0; 3];
            if 1.0 > hp {
                let v = (hp * 0.0625) * 0.25 + 0.00390625;
                spin_limit[0] = v;
                spin_limit[1] = v;
            }
            IkParams { length: ARM_LENGTH, twist, max_turn, clamp_max: 0.2945243, pose_spin: [0.875; 3], spin_limit, flags }
        };
        three_bone_ik(h, bodies, 2, first, target, &IDENTITY, Vec3::ZERO, &params, &mut end_rot);
        if state == 2 {
            let lean = (h.bones[0].rot[2].y as f64).asin() as f32;
            let lean = if lean > 0.0 { lean * 4.0 } else { lean * 0.25 };
            let (y, z) = if lean as f64 > 2.748893571891069 {
                (0.45472196, -0.18835203)
            } else {
                let (s, c) = (lean as f64).sin_cos();
                (((-0.65625 * c) * 0.75) as f32, ((0.65625 * -s) * 0.75) as f32)
            };
            let lean = if lean as f64 > 2.748893571891069 { 2.7488935 } else { lean };
            let target = Vec3::new(if k == 0 { -0.09375 } else { 0.09375 }, y, z);
            let twist = if lean > 0.0 { lean * 0.5 } else { 0.0 };
            let twist = if k == 0 { -twist } else { twist };
            let params = IkParams { twist, spin_limit: [0.0625; 3], ..params };
            three_bone_ik(h, bodies, 2, first, target, &IDENTITY, Vec3::ZERO, &params, &mut end_rot);
        }
    }

    for bone in [1, 2, 3, 4, 5, 7, 8] {
        if let Some(id) = h.bones[bone].joint
            && let Some(Bond::Joint(j)) = bodies.bond_mut(id)
        {
            j.limit_active = false;
        }
    }
    if state == 2 {
        for (parent, child, spin_limit) in [(0, 1, 0.0078125), (1, 2, 0.0078125), (2, 3, 0.0078125), (2, 4, 0.03125), (4, 5, 0.03125), (2, 7, 0.03125), (7, 8, 0.03125)] {
            limit_joint(h, bodies, parent, child, spin_limit);
        }
    }
    if h.pain > 0 {
        let p = h.pain as f32 / 60.0;
        let limit = if p > 1.0 { 0.0078125 } else { 0.125 - (p * 0.125) * 0.9375 };
        for (parent, child, spin_limit) in [(0, 1, limit), (1, 2, limit), (2, 3, limit), (2, 4, limit * 0.25), (4, 5, limit * 0.25), (2, 7, limit * 0.25), (7, 8, limit * 0.25)] {
            limit_joint(h, bodies, parent, child, spin_limit);
        }
    }
}

/// The right arm's reach when pointing: ahead and across the chest, turned by the view's pitch (free_look_pitch) and yaw
/// (free_look_yaw), each kept within limits.
fn point_target(h: &Human) -> Vec3 {
    let clamp = |turn: f32| {
        let a = POINT_TURN * turn;
        if POINT_MIN_TURN_F64 > a as f64 {
            POINT_MIN_TURN
        } else if a as f64 > POINT_MAX_TURN_F64 {
            POINT_MAX_TURN
        } else {
            a
        }
    };
    let target = rotate_vector_about_axis(POINT_REACH, Vec3::X, clamp(h.free_look_pitch));
    rotate_vector_about_axis(target, Vec3::Y, clamp(h.free_look_yaw))
}

/// Paper (a newspaper or document) is held open in both hands: the left arm takes its second grip.
fn is_paper(kind: u32) -> bool {
    matches!(kind, 0x22 | 0x1d)
}

/// The item an arm holds and which of the item's grips it holds it by: the right arm holds the right hand's item,
/// the left arm the left hand's or else the second grip of a two-handed item in the right hand; the left arm holds
/// paper by its second grip.
fn held_item(h: &Human, touch: &Touchables, k: usize) -> Option<(usize, usize)> {
    let (item, grip) = arm_item(h, touch, k)?;
    let paper = touch.items.get(item).is_some_and(|i| is_paper(i.item_type as u32));
    Some((item, if paper && k & 1 == 0 { 1 } else { grip }))
}

fn arm_item(h: &Human, touch: &Touchables, k: usize) -> Option<(usize, usize)> {
    let slots = &h.inventory;
    if k == 1 || slots[1].count > 0 {
        let slot = &slots[if k == 1 { 0 } else { 1 }];
        return (slot.count > 0).then_some((slot.items[0] as usize, 0));
    }
    if slots[0].count <= 0 {
        return None;
    }
    let item = slots[0].items[0] as usize;
    let kind = touch.items.get(item)?.item_type as usize;
    if (touch.types[kind].hands <= 1 && kind != 0x24) || (!(h.bones[0].rot[1].y > 0.707) && h.input_flags & 8 != 0) {
        return None;
    }
    Some((item, 1))
}

/// The hold matrix and hand position of an arm holding an item (the item part of human_calculate_arm_angles): the
/// matrix follows the view pitch (for a gun, the aim `aim`), turned `yaw` while the other hand aims; the hand sits
/// below and in front of the shoulder (a gun at the right shoulder), moved by the item's grip and pulled against the
/// item's motion relative to the torso.
#[allow(clippy::too_many_arguments)]
fn hold_frame(h: &Human, bodies: &RigidBodies, touch: &Touchables, k: usize, item_id: usize, hand: usize, yaw: f32, aim: Option<[f32; 4]>) -> (RotMatrix, Vec3) {
    let item = touch.items.get(item_id).unwrap();
    let ty = &touch.types[item.item_type as usize];
    let mut m = IDENTITY;
    rotate_orientation(&mut m, Vec3::X, 0.5 * h.view_pitch - 0.049087387);
    let axis = m[0];
    rotate_orientation(&mut m, axis, 0.0061359233);
    let axis = m[1];
    rotate_orientation(&mut m, axis, -0.01840777);
    if h.action_type == 1 {
        let axis = m[2];
        rotate_orientation(&mut m, axis, 0.3926991);
    }
    if h.vehicle.is_none() && h.input_flags & 2 != 0 {
        rotate_orientation(&mut m, Vec3::X, 0.7853982);
    }
    let throwing = h.input_flags & 0x20 != 0 || h.action_type == 0;
    if throwing {
        let tp = h.throw_pitch;
        rotate_orientation(&mut m, Vec3::X, tp + tp);
    }
    rotate_orientation(&mut m, Vec3::Y, yaw);
    if ty.is_gun
        && let Some(q) = aim
    {
        m = quaternion_to_rot_matrix([-q[0], -q[1], -q[2], q[3]]);
        let axis = m[0];
        rotate_orientation(&mut m, axis, (h.view_pitch as f64 - GUN_PITCH_OFFSET) as f32);
        if h.input_flags & 2 != 0 {
            let axis = m[0];
            rotate_orientation(&mut m, axis, 0.7853982);
        }
        if throwing {
            let (axis, tp) = (m[0], h.throw_pitch);
            rotate_orientation(&mut m, axis, tp + tp);
        }
    }
    let mut v = if h.movement_mode == 2 { Vec3::new(0.0625, 0.125, 0.0) } else { Vec3::new(0.1875, 0.0625, 0.0) };
    if k == 0 && h.inventory[1].count > 0 {
        v = Vec3::new(-0.125, 0.125, 0.0);
    }
    let paper = is_paper(item.item_type as u32);
    if paper {
        v = if h.vehicle.is_none() { Vec3::ZERO } else { Vec3::new(0.0, -0.125, 0.0) };
    }
    let [m0, m1, r2] = m;
    if throwing {
        let z = (h.throw_pitch as f64 + 1.1780972450962501) as f32;
        let mut k = -0.25f32;
        if !(z > 0.0) {
            k -= ((((z * 3.0) as f64).sin()) * 0.375) as f32;
        }
        v = Vec3::new(v.x + k * r2.x, v.y + k * r2.y, 0.0 + k * r2.z);
    }
    v = Vec3::new(
        ((-0.1875 * r2.x + v.x) + -0.375 * r2.x) + 0.0625 * m1.x,
        ((-0.1875 * r2.y + v.y) + -0.375 * r2.y) + 0.0625 * m1.y,
        ((-0.1875 * r2.z + v.z) + -0.375 * r2.z) + 0.0625 * m1.z,
    );
    // TODO: the per-type hold poses (computers, phones, briefcases, grenades, cash, disks, keys and doors)
    if item.item_type as u32 == 0xb && h.vehicle.is_none() {
        v = if h.movement_mode == 2 {
            Vec3::new(-0.25 * r2.x + v.x, -0.25 * r2.y + v.y, -0.25 * r2.z + v.z)
        } else {
            Vec3::new(m1.x * -0.125 + v.x, m1.y * -0.125 + v.y, -0.125 * m1.z + v.z)
        };
    }
    if k == 0 && h.inventory[1].count > 0 && !paper {
        v = Vec3::new(r2.x * 0.125 + v.x, r2.y * 0.125 + v.y, 0.125 * r2.z + v.z);
    }
    let p = ty.hold_pos[hand];
    v = Vec3::new(
        ((p.x * m0.x + v.x) + p.y * m1.x) + r2.x * p.z,
        r2.y * p.z + ((m0.y * p.x + v.y) + m1.y * p.y),
        p.z * r2.z + ((m0.z * p.x + v.z) + m1.z * p.y),
    );
    if ty.is_gun && item.item_type as u32 != 0xb && (k | hand) != 0 {
        v = gun_hold_pos(h, ty, hand, &m);
    }
    if h.action_type == 1 {
        let a = m[0];
        let t = -(((v.x * a.x + v.y * a.y) + v.z * a.z) * h.action_progress);
        v = Vec3::new(a.x * t + v.x, a.y * t + v.y, t * a.z + v.z);
    }
    // TODO: the binary skips this pull while the global byte at 0x5b08aec0 is set
    let vel = bodies.get(item.body).map_or(Vec3::ZERO, |b| b.vel);
    let tv = h.locomotion.torso_vel;
    let d = Vec3::new(vel.x - tv.x, vel.y - tv.y, vel.z - tv.z);
    let [c0, c1, c2] = h.bones[2].rot;
    let l = Vec3::new((c0.x * d.x + c0.y * d.y) + c0.z * d.z, (c1.x * d.x + c1.y * d.y) + c1.z * d.z, (d.x * c2.x + d.y * c2.y) + d.z * c2.z);
    if h.input_flags & 0x20 == 0 && h.action_type != 0 {
        v = Vec3::new(l.x * -0.75 + v.x, l.y * -0.75 + v.y, l.z * -0.75 + v.z);
    }
    let r = ty.hold_rot[hand];
    if r[3].abs() > 0.0 {
        let [m0, m1, m2] = m;
        let axis = Vec3::new((m0.x * r[0] + m1.x * r[1]) + m2.x * r[2], (m0.y * r[0] + m1.y * r[1]) + m2.y * r[2], (m0.z * r[0] + m1.z * r[1]) + m2.z * r[2]);
        rotate_orientation(&mut m, axis, -r[3]);
    }
    (m, v)
}

/// get_item_hold_pos_rot: where an arm holds a gun by `hand`: in front of the right shoulder (lower unless zoomed in
/// or busy with the inventory), back along the hold matrix `m` by the gun's holding offset and out to the grip.
fn gun_hold_pos(h: &Human, ty: &ItemType, hand: usize, m: &RotMatrix) -> Vec3 {
    let j = BONES[7].joint;
    let mut y = j.y;
    if h.action_type != 1 && h.movement_mode <= 1 {
        y -= 0.25;
    }
    if h.vehicle.is_some() {
        y += 0.25;
    }
    let (g, p) = (ty.gun_hold_pos, ty.hold_pos[hand]);
    let along = |v: Vec3, k: [f32; 3]| m.iter().zip(k).fold(v, |v, (r, k)| Vec3::new(v.x + r.x * k, v.y + r.y * k, v.z + k * r.z));
    along(along(Vec3::new(j.x - 0.0625, y, j.z - 0.09375), [-g.x, -g.y, -g.z]), [p.x, p.y, p.z])
}

/// human_arm_item_collision: the right upper arm and the chest push a gun held in the right hand out of them, each
/// against the gun's barrel just either side of its holding point.
fn arm_item_collision(h: &Human, bodies: &mut RigidBodies, touch: &Touchables, item_id: usize) {
    let item = touch.items.get(item_id).unwrap();
    let ty = &touch.types[item.item_type as usize];
    let Some(body) = bodies.get(item.body) else { return };
    let (pos, [_, r1, r2]) = item.pocket_pose.unwrap_or((body.pos, body.rot));
    let item_body = item.body;
    let g = ty.gun_hold_pos;
    let base = Vec3::new(r2.x * g.z + pos.x, r2.y * g.z + pos.y, g.z * r2.z + pos.z);
    for i in 0..4 {
        let bone = if i <= 1 { 7 } else { 2 };
        let (b, t) = (&h.bones[bone], &BONES[bone]);
        let (a, p) = (b.rot[t.shape as usize], b.pos);
        let (start, end) = if i > 1 {
            let back = (-t.shape_size[0]) * 0.5 - 0.0625;
            let front = BONES[7].joint.x - 0.125;
            let u = b.rot[1];
            let (ux, uy, uz) = (u.x * 0.0625, u.y * 0.0625, u.z * 0.0625);
            (
                Vec3::new((back * a.x + p.x) + ux, (back * a.y + p.y) + uy, (back * a.z + p.z) + uz),
                Vec3::new((p.x + a.x * front) + ux, (p.y + a.y * front) + uy, (front * a.z + p.z) + uz),
            )
        } else {
            (
                Vec3::new(0.2890625 * a.x + p.x, 0.2890625 * a.y + p.y, 0.2890625 * a.z + p.z),
                Vec3::new(0.1640625 * a.x + p.x, 0.1640625 * a.y + p.y, 0.1640625 * a.z + p.z),
            )
        };
        let gy = if i & 1 != 0 { g.y + 0.0625 } else { g.y - 0.0625 };
        let grip = Vec3::new(base.x + r1.x * gy, base.y + r1.y * gy, base.z + r1.z * gy);
        let (hit, on_arm, on_gun, dist) = segment_closest_points(start, end, grip, pos, GUN_CONTACT_RADIUS);
        if !hit {
            continue;
        }
        let d = Vec3::new(on_arm.x - on_gun.x, on_arm.y - on_gun.y, on_arm.z - on_gun.z);
        let len = ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt();
        let n = if len != 0.0 {
            let inv = 1.0 / len;
            Vec3::new(d.x * inv, d.y * inv, inv * d.z)
        } else {
            Vec3::ZERO
        };
        let mid = Vec3::new((on_arm.x + on_gun.x) * 0.5, (on_arm.y + on_gun.y) * 0.5, 0.5 * (on_arm.z + on_gun.z));
        let off_arm = Vec3::new(mid.x - p.x, mid.y - p.y, mid.z - p.z);
        let off_gun = Vec3::new(mid.x - pos.x, mid.y - pos.y, mid.z - pos.z);
        bodies.add_body_contact(b.body, item_body, off_arm, off_gun, n, GUN_CONTACT_RADIUS - dist, GUN_FRICTION, GUN_DEPTH_SCALE, GUN_SOFTNESS);
    }
}

/// bond_attach_item_to_human_bone: for this tick, a point bond pulls the item's grip onto the hand and an angular
/// bond turns the item towards its hold orientation in the hand.
fn attach_item_to_bone(h: &Human, bodies: &mut RigidBodies, touch: &Touchables, bone: usize, item_id: usize, hand: usize) {
    let item = touch.items.get(item_id).unwrap();
    let kind = item.item_type as u32;
    if kind == 0x27 {
        return;
    }
    let ty = &touch.types[kind as usize];
    let body_a = h.bones[bone].body;
    bodies.create_bond(Bond::ItemPoint(ItemPoint::new(body_a, item.body, Vec3::ZERO, ty.hold_pos[hand])));
    if kind == 0x1c {
        return;
    }
    let Some(item_body) = bodies.get(item.body) else { return };
    let [b0, b1, b2] = h.bones[bone].rot;
    // the item record's orientation: the body's, or for an item just taken out of a pocket its last snap
    let [i0, i1, i2] = item.pocket_pose.map_or(item_body.rot, |(_, rot)| rot);
    let d = |a: Vec3, b: Vec3| (a.x * b.x + a.y * b.y) + a.z * b.z;
    let mut m = [Vec3::new(d(i0, b0), d(i1, b0), d(b0, i2)), Vec3::new(d(b1, i0), d(b1, i1), d(b1, i2)), Vec3::new(d(i0, b2), d(i1, b2), d(b2, i2))];
    let r = ty.hold_rot[hand];
    if r[3].abs() > 0.0 {
        rotate_orientation(&mut m, Vec3::new(r[0], r[1], r[2]), r[3]);
    }
    let q = rot_matrix_to_quaternion(&m);
    let (axis, angle) = quaternion_to_axis_angle([-q[0], -q[1], -q[2], q[3]]);
    let v = Vec3::new(axis.x * angle, axis.y * angle, axis.z * angle);
    let w = Vec3::new((v.y * b1.x + v.x * b0.x) + v.z * b2.x, (b0.y * v.x + b1.y * v.y) + b2.y * v.z, (b0.z * v.x + b1.z * v.y) + b2.z * v.z);
    bodies.create_bond(Bond::ItemAngular(ItemAngular::new(body_a, item.body, Vec3::new(w.x * 0.25, w.y * 0.25, w.z * 0.25))));
}

/// Crouching while lying on the ground: the arm reaches along the chest's up direction towards the ground below its
/// shoulder.
fn crouch_reach(h: &Human, map: &Map, first: usize) -> Vec3 {
    let chest = &h.bones[2];
    let [r0, r1, r2] = chest.rot;
    let up = Vec3::Y;
    let d = Vec3::new((r0.x * up.x + r0.y * up.y) + r0.z * up.z, (r1.x * up.x + r1.y * up.y) + r1.z * up.z, (up.y * r2.y + up.x * r2.x) + up.z * r2.z);
    let len = (d.z * d.z + (d.x * d.x + d.y * d.y)).sqrt();
    let d = if len == 0.0 {
        Vec3::ZERO
    } else {
        let inv = 1.0 / len;
        Vec3::new(d.x * inv, d.y * inv, inv * d.z)
    };
    let j = BONES[first].joint;
    let p = chest.pos;
    let shoulder = Vec3::new(((j.x * r0.x + p.x) + j.y * r1.x) + r2.x * j.z, ((r1.y * j.y) + (r0.y * j.x + p.y)) + r2.y * j.z, ((r1.z * j.y) + (r0.z * j.x + p.z)) + j.z * r2.z);
    let end = Vec3::new(shoulder.x - up.x, shoulder.y - up.y, shoulder.z - up.z);
    let mut k = -0.65625;
    if let Some(hit) = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, shoulder, end).map(|h| h.hit.pos) {
        let v = (shoulder.y - hit.y) + 0.00390625;
        if !(v > 0.65625) {
            k = -v;
        }
    }
    Vec3::new(d.x * k, d.y * k, k * d.z)
}

/// Clamps `child` to its joint limits relative to `parent` (human_accumulate_joint_limit_correction into the joint's
/// bond) and sets the joint's spin limit.
fn limit_joint(h: &mut Human, bodies: &mut RigidBodies, parent: usize, child: usize, spin_limit: f32) {
    let (correction, angles) = super::physics::joint_limit_correction(h, parent, child);
    h.bones[child].limit_angles = angles;
    if let Some(id) = h.bones[child].joint
        && let Some(Bond::Joint(j)) = bodies.bond_mut(id)
    {
        j.limit_correction = correction;
        let len = ((correction.x * correction.x + correction.y * correction.y) + correction.z * correction.z).sqrt();
        j.limit_active = len > 0.0;
        j.spin_limit = spin_limit;
    }
}
