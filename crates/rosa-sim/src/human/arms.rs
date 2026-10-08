use glam::Vec3;
use crate::{sim::items::Touchables, world::map::Map};
use rosa_physics::{
    Bond, ItemAngular, ItemPoint, RigidBodies, RotMatrix,
    rotation::{IDENTITY, multiply_matrixes, quaternion_multiply, quaternion_to_axis_angle, rot_matrix_to_quaternion, rotate_orientation},
};

use super::{
    Human,
    bones::BONES,
    locomotion::trace,
    ik::{IK_LIMIT, IK_MIRROR, IkParams, three_bone_ik},
};

const SPINE_TURN: f32 = 0.3926991;
const SWAY_SCALE: f32 = 0.0078125;
const ARM_LENGTH: f32 = 0.65625;

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

/// Turns `child` (relative to `parent`) towards the pose `pose`, with the damping term `damping`, and rebuilds the
/// predicted orientation `frame`.
fn drive_spine_joint(h: &Human, bodies: &mut RigidBodies, parent: usize, child: usize, pose: [f32; 4], damping: Vec3, frame: &mut RotMatrix) {
    let m = multiply_matrixes(&h.bones[child].rot, &h.bones[parent].rot);
    let mut q = rot_matrix_to_quaternion(&m);
    let dot = ((pose[0] * q[0] + pose[1] * q[1]) + pose[2] * q[2]) + pose[3] * q[3];
    if 0.0 > dot {
        q = q.map(|c| -c);
    }
    let q = [-q[0], -q[1], -q[2], q[3]];
    let (axis, angle) = quaternion_to_axis_angle(quaternion_multiply(q, pose));
    let angle = angle.clamp(-SPINE_TURN, SPINE_TURN);
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
    // TODO: guns, throwing, two-handed and phone poses, vehicles and the aim offsets they use
    if h.action_type != 0 && h.input_flags & 0x20 != 0 {
        h.throw_pitch = 8.0 * h.unk_16c;
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

    let has_gun = false;
    let mut aim = if h.movement_mode == 2 { 0.7853982 } else { SPINE_TURN };
    if h.input_flags & 2 != 0 || !has_gun {
        aim = 0.0;
    }

    let (vehicle_yaw, vehicle_pitch) = (0.0f32, 0.0f32);
    let body_yaw = (((vehicle_yaw + h.unk_168 * 0.5) + h.yaw_offset) + aim) * 0.5;
    let aiming = h.input_flags & 0x20 != 0;

    let identity = [0.0, 0.0, 0.0, 1.0];
    let mut frames = [IDENTITY; 3];
    let waist_pitch = if aiming { 0.5 * h.view_pitch } else { h.view_pitch * 0.75 + 0.75 * h.unk_16c };
    let waist_pitch = if h.vehicle.is_some() { vehicle_pitch } else { waist_pitch } * 0.75;
    let mut frame = IDENTITY;
    let pose = add_turn(identity, &mut frame, 1, body_yaw);
    let pose = add_turn(pose, &mut frame, 0, waist_pitch);
    drive_spine_joint(h, bodies, 0, 1, pose, relative_spin_damping(h, 0, 1), &mut frames[0]);

    let chest_pitch = if aiming { 0.5 * h.view_pitch } else { 0.25 * h.view_pitch + 0.75 * h.unk_16c };
    let chest_pitch = if h.vehicle.is_some() { vehicle_pitch } else { chest_pitch } * 0.5;
    let mut frame = IDENTITY;
    let pose = add_turn(identity, &mut frame, 0, chest_pitch);
    let pose = add_turn(pose, &mut frame, 1, body_yaw);
    let chest_damping = relative_spin_damping(h, 1, 2);
    drive_spine_joint(h, bodies, 1, 2, pose, chest_damping, &mut frames[1]);

    let mut frame = IDENTITY;
    let pose = add_turn(identity, &mut frame, 1, 0.5 * h.unk_168 - aim);
    let pose = add_turn(pose, &mut frame, 0, 0.5 * h.view_pitch + 0.25 * h.unk_16c);
    // TODO: the binary damps the head with the chest's relative spin, not the head's
    drive_spine_joint(h, bodies, 2, 3, pose, chest_damping, &mut frames[2]);
    h.locomotion.spine_frames = frames;

    let state = h.movement_state;
    for k in 0..2 {
        let first = 4 + 3 * k;
        let held = held_item(h, touch, k);
        let hold = held.map(|(item, hand)| hold_frame(h, bodies, touch, k, item, hand));
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
            angle = (1.570796326795 - (h.view_pitch + h.unk_16c) as f64) as f32;
            scale = 0.9375;
        }
        let a = angle as f64;
        let y = ((-0.65625 * a.cos()) * scale as f64) as f32;
        let z = (scale as f64 * (-a.sin() * 0.65625)) as f32;
        let mut target = match hold {
            Some((m, v)) => {
                let j = BONES[first].joint;
                let m1 = m[1];
                Vec3::new((0.0625 * m1.x + v.x) - j.x, (0.0625 * m1.y + v.y) - j.y, (0.0625 * m1.z + v.z) - j.z)
            }
            None => Vec3::new(if k == 0 { -0.0625 } else { 0.0625 }, y, z),
        };
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
        // TODO: punching or aiming with one arm (input flags 1 and 2)
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
            // TODO: human_arm_item_collision for a gun held in the right arm
            continue;
        }
        let mut flags = IK_MIRROR + (1.0 > hp) as u32;
        let airborne = (state == 1 && h.input_flags & 8 == 0) || (state == 3 && !(0.125 + h.locomotion.feet[0].swing_phase < 1.0));
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

/// The item an arm holds and which of the item's grips it holds it by: the right arm holds the right hand's item,
/// the left arm the left hand's or else the second grip of a two-handed item in the right hand.
fn held_item(h: &Human, touch: &Touchables, k: usize) -> Option<(usize, usize)> {
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
    // TODO: the AK and M16 types (0x22, 0x1d) take their second grip in the left arm as well
    Some((item, 1))
}

/// The hold matrix and hand position of an arm holding an item (the item part of human_calculate_arm_angles): the
/// matrix follows the view pitch, the hand sits below and in front of the shoulder, moved by the item's grip and
/// pulled against the item's motion relative to the torso.
fn hold_frame(h: &Human, bodies: &RigidBodies, touch: &Touchables, k: usize, item_id: usize, hand: usize) -> (RotMatrix, Vec3) {
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
    // TODO: a gun in the right hand turns the hold by the aim angle instead of -0
    rotate_orientation(&mut m, Vec3::Y, -0.0);
    let mut v = if h.movement_mode == 2 { Vec3::new(0.0625, 0.125, 0.0) } else { Vec3::new(0.1875, 0.0625, 0.0) };
    if k == 0 && h.inventory[1].count > 0 {
        v = Vec3::new(-0.125, 0.125, 0.0);
    }
    // TODO: guns hold at their own offsets
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
    if k == 0 && h.inventory[1].count > 0 {
        v = Vec3::new(r2.x * 0.125 + v.x, r2.y * 0.125 + v.y, 0.125 * r2.z + v.z);
    }
    let p = ty.hold_pos[hand];
    v = Vec3::new(
        ((p.x * m0.x + v.x) + p.y * m1.x) + r2.x * p.z,
        r2.y * p.z + ((m0.y * p.x + v.y) + m1.y * p.y),
        p.z * r2.z + ((m0.z * p.x + v.z) + m1.z * p.y),
    );
    // TODO: the inventory animation (action 1) slides the hand along the hold matrix by its progress
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
    if let Some(hit) = trace(map, shoulder, end) {
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
