use glam::Vec3;
use rosa_physics::{
    Bond, RigidBodies, RotMatrix,
    rotation::{IDENTITY, multiply_matrixes, quaternion_multiply, quaternion_to_axis_angle, rot_matrix_to_quaternion, rotate_orientation},
};

use super::{Human, bones::BONES};

const KNEE_AXIS: Vec3 = Vec3::X;

pub const IK_MIRROR: u32 = 4;
pub const IK_FIXED_END: u32 = 8;
pub const IK_LIMIT: u32 = 0x10;
pub const IK_END_ORIENTATION: u32 = 2;

/// The per-call settings of human_update_three_bone_ik_constraints.
pub struct IkParams {
    pub length: f32,
    pub twist: f32,
    pub max_turn: f32,
    pub clamp_max: f32,
    pub pose_spin: [f32; 3],
    pub spin_limit: [f32; 3],
    pub flags: u32,
}

/// human_clamp_bone_relative_correction: limits how far `v` may differ from the bones' current relative spin.
pub fn clamp_bone_relative_correction(h: &Human, a: usize, b: usize, v: &mut Vec3, max: f32, k: f32) {
    let (wa, wb) = (h.bones[a].ang_vel, h.bones[b].ang_vel);
    let d = Vec3::new((wa.x - wb.x) * k, (wa.y - wb.y) * k, k * (wa.z - wb.z));
    let c = Vec3::new(v.x - d.x, v.y - d.y, v.z - d.z);
    let len = ((c.x * c.x + c.y * c.y) + c.z * c.z).sqrt();
    if len > max {
        let n = if len == 0.0 {
            Vec3::ZERO
        } else {
            let inv = 1.0 / len;
            Vec3::new(c.x * inv, c.y * inv, c.z * inv)
        };
        *v = Vec3::new(d.x + n.x * max, n.y * max + d.y, n.z * max + d.z);
    }
}

/// human_limit_relative_joint_rotation: nudges `v` (in world space) back when the child bone leaves its joint limits.
pub fn limit_relative_joint_rotation(h: &Human, parent: usize, child: usize, v: &mut Vec3) {
    let [p0, p1, p2] = h.bones[child].rot;
    let [c0, c1, c2] = h.bones[parent].rot;
    let d = |a: Vec3, b: Vec3| (a.x * b.x + a.y * b.y) + a.z * b.z;
    let m = [
        Vec3::new(d(p0, c0), d(p1, c0), d(c0, p2)),
        Vec3::new(d(c1, p0), d(c1, p1), d(c1, p2)),
        Vec3::new(d(p0, c2), d(p1, c2), d(c2, p2)),
    ];
    let mut q = rot_matrix_to_quaternion(&m);
    let mut l = [(v.y * c0.y + v.x * c0.x) + c0.z * v.z, (v.x * c1.x + v.y * c1.y) + v.z * c1.z, (v.y * c2.y + v.x * c2.x) + v.z * c2.z];
    if 0.0 > q[3] {
        q = q.map(|c| -c);
    }
    let mut a = [q[0] + q[0], q[1] + q[1], q[2] + q[2]];
    if (child == 11 || child == 14) && (a[0] as f64) > 1.570796326795 {
        a[0] = (-3.14159265359 - (3.14159265359 - a[0] as f64)) as f32;
    }
    let t = &BONES[child];
    let (lo, hi) = (t.limit_min.to_array(), t.limit_max.to_array());
    for k in 0..3 {
        if lo[k] > a[k] {
            let x = -(a[k] - lo[k]) * 0.25 - l[k];
            if x > 0.0 {
                l[k] += x;
            }
        }
        if a[k] > hi[k] {
            let x = -(a[k] - hi[k]) * 0.25 - l[k];
            if 0.0 > x {
                l[k] += x;
            }
        }
    }
    *v = Vec3::new((c0.x * l[0] + c1.x * l[1]) + c2.x * l[2], (c0.y * l[0] + c1.y * l[1]) + c2.y * l[2], (l[0] * c0.z + l[1] * c1.z) + l[2] * c2.z);
}

fn conjugate_towards(m: [f32; 4], flip: bool) -> [f32; 4] {
    if flip { [m[0], m[1], m[2], -m[3]] } else { [-m[0], -m[1], -m[2], m[3]] }
}

fn dot4(a: [f32; 4], b: [f32; 4]) -> f32 {
    ((a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]) + a[3] * b[3]
}

fn clamp_turn(axis: Vec3, angle: f32, max: f32) -> Vec3 {
    let neg = -max;
    let a = if neg > angle { neg } else { angle };
    let lim = if max < a { max } else { a };
    Vec3::new(axis.x * lim, axis.y * lim, lim * axis.z)
}

fn set_joint(h: &Human, bodies: &mut RigidBodies, bone: usize, target: Vec3, spin_limit: f32, pose_spin: f32) {
    let Some(id) = h.bones[bone].joint else { return };
    if let Some(Bond::Joint(j)) = bodies.bond_mut(id) {
        j.target_ang_vel = target;
        j.limit_active = false;
        j.spin_limit = spin_limit;
        j.pose_spin = pose_spin;
    }
}

/// human_update_three_bone_ik_constraints: drives the joints of a three-bone chain (thigh-shin-foot or
/// upper arm-forearm-hand, starting at `first` under `root`) towards reaching `target`, given in `frame` space.
#[allow(clippy::too_many_arguments)]
pub fn three_bone_ik(h: &mut Human, bodies: &mut RigidBodies, root: usize, first: usize, target: Vec3, frame: &RotMatrix, correction: Vec3, p: &IkParams, end_rot: &mut [f32; 4]) {
    let t_len = ((target.x * target.x + target.y * target.y) + target.z * target.z).sqrt();
    let l_abs = p.length.abs();
    let c = (if l_abs < t_len { l_abs } else { t_len }) / p.length;
    let a = -(c as f64).acos();
    let mut bend = (a + a) as f32;
    if p.flags & IK_MIRROR != 0 {
        bend = -bend;
    }

    let m = multiply_matrixes(&h.bones[first].rot, &h.bones[root].rot);
    let qm = rot_matrix_to_quaternion(&m);
    let [f0, f1, f2] = *frame;
    let tl_x = (f0.x * target.x + f0.y * target.y) + f0.z * target.z;
    let mut tl_y = (f1.x * target.x + f1.y * target.y) + f1.z * target.z;
    let tl_z = (target.y * f2.y + target.x * f2.x) + target.z * f2.z;
    let yy = if p.flags & IK_MIRROR == 0 && tl_y > -0.0625 {
        tl_y = -0.0625;
        0.00390625
    } else {
        tl_y * tl_y
    };
    let dl = ((tl_x * tl_x + yy) + tl_z * tl_z).sqrt();
    let dir = if dl == 0.0 {
        Vec3::ZERO
    } else {
        let inv = 1.0 / dl;
        Vec3::new(-tl_x * inv, -tl_y * inv, inv * -tl_z)
    };
    let g = KNEE_AXIS;
    let e = Vec3::new(g.y * dir.z - g.z * dir.y, g.z * dir.x - dir.z * g.x, g.x * dir.y - g.y * dir.x);
    let el = ((e.x * e.x + e.y * e.y) + e.z * e.z).sqrt();
    let e = if el == 0.0 {
        Vec3::ZERO
    } else {
        let inv = 1.0 / el;
        Vec3::new(e.x * inv, e.y * inv, inv * e.z)
    };
    let f = Vec3::new(dir.y * e.z - dir.z * e.y, dir.z * e.x - e.z * dir.x, e.y * dir.x - dir.y * e.x);
    let fl = ((f.x * f.x + f.y * f.y) + f.z * f.z).sqrt();
    let f = if fl == 0.0 {
        Vec3::ZERO
    } else {
        let inv = 1.0 / fl;
        Vec3::new(f.x * inv, f.y * inv, inv * f.z)
    };
    let mut r = [Vec3::new(f.x, dir.x, e.x), Vec3::new(f.y, dir.y, e.y), Vec3::new(f.z, dir.z, e.z)];
    rotate_orientation(&mut r, Vec3::Y, p.twist);
    let half = -bend * 0.5;
    rotate_orientation(&mut r, KNEE_AXIS, half);
    let q = rot_matrix_to_quaternion(&r);
    let mut q_plane = q;
    if p.flags & IK_FIXED_END != 0 {
        let mut r2 = IDENTITY;
        rotate_orientation(&mut r2, Vec3::Y, p.twist);
        rotate_orientation(&mut r2, KNEE_AXIS, half);
        q_plane = rot_matrix_to_quaternion(&r2);
    }

    let half_corr = |v: Vec3, t: Vec3| (0.5 * v.x + t.x, t.y + 0.5 * v.y, t.z + 0.5 * v.z);

    let qm2 = conjugate_towards(qm, 0.0 > dot4(q, qm));
    let (axis, angle) = quaternion_to_axis_angle(quaternion_multiply(qm2, q));
    let av = clamp_turn(axis, angle, p.max_turn);
    let [b0, b1, b2] = h.bones[first].rot;
    let tx = (b2.x * av.z) + (b0.x * av.x + b1.x * av.y);
    let ty = (b0.y * av.x + b1.y * av.y) + b2.y * av.z;
    let tz = (av.x * b0.z + av.y * b1.z) + av.z * b2.z;
    let (mut x, mut y, mut z) = half_corr(correction, Vec3::new(tx, ty, tz));
    if first == 10 || first == 13 {
        let ft = h.locomotion.foot_torque[if first == 10 { 0 } else { 1 }];
        x += ft.x;
        y += ft.y;
        z += ft.z;
    }
    let mut t1 = Vec3::new(x, y, z);
    if p.flags & IK_LIMIT != 0 {
        clamp_bone_relative_correction(h, root, first, &mut t1, p.clamp_max, 1.0);
        if let Some(id) = h.bones[first].joint
            && let Some(Bond::Joint(j)) = bodies.bond(id)
        {
            let mut lim = j.pose_limit;
            limit_relative_joint_rotation(h, root, first, &mut lim);
            if let Some(Bond::Joint(j)) = bodies.bond_mut(id) {
                j.pose_limit = lim;
            }
        }
    }
    set_joint(h, bodies, first, t1, p.spin_limit[0], p.pose_spin[0]);

    let mid = first + 1;
    let m2 = multiply_matrixes(&h.bones[mid].rot, &h.bones[first].rot);
    let qm = rot_matrix_to_quaternion(&m2);
    let (s, c) = ((0.5 * bend) as f64).sin_cos();
    let (s, c) = (s as f32, c as f32);
    let q2 = [s, 0.0, 0.0, c];
    let dot = ((s * qm[0] + qm[1] * 0.0) + qm[2] * 0.0) + c * qm[3];
    let qm2 = conjugate_towards(qm, 0.0 > dot && 0.0 > c * qm[3]);
    let (axis, angle) = quaternion_to_axis_angle(quaternion_multiply(qm2, q2));
    let av = clamp_turn(axis, angle, p.max_turn);
    let [b0, b1, b2] = h.bones[mid].rot;
    let tx = (b2.x * av.z) + (av.x * b0.x + b1.x * av.y);
    let ty = (b0.y * av.x + b1.y * av.y) + b2.y * av.z;
    let tz = (av.x * b0.z + av.y * b1.z) + av.z * b2.z;
    let (x, y, z) = half_corr(correction, Vec3::new(tx, ty, tz));
    set_joint(h, bodies, mid, Vec3::new(x, y, z), p.spin_limit[1], p.pose_spin[1]);

    let end = first + 2;
    let m3 = multiply_matrixes(&h.bones[end].rot, &h.bones[mid].rot);
    let qm = rot_matrix_to_quaternion(&m3);
    let mut qf = [0.0, 0.0, 0.0, 1.0];
    let conj2 = [-s, -0.0, -0.0, c];
    let conj_plane = [-q_plane[0], -q_plane[1], -q_plane[2], q_plane[3]];
    if p.flags & IK_END_ORIENTATION != 0 {
        qf = quaternion_multiply(conj2, conj_plane);
        if 0.0 > dot4(qf, *end_rot) {
            *end_rot = end_rot.map(|c| -c);
        }
        qf = quaternion_multiply(qf, *end_rot);
        if p.flags & IK_FIXED_END != 0 {
            qf = quaternion_multiply([s, 0.0, 0.0, c], q_plane);
        }
    } else if p.flags & IK_FIXED_END != 0 {
        qf = quaternion_multiply(conj2, conj_plane);
    }
    let qm2 = conjugate_towards(qm, 0.0 > dot4(qf, qm));
    let (axis, angle) = quaternion_to_axis_angle(quaternion_multiply(qm2, qf));
    let av = clamp_turn(axis, angle, p.max_turn);
    let [b0, b1, b2] = h.bones[end].rot;
    let tx = (b0.x * av.x + b1.x * av.y) + b2.x * av.z;
    let ty = (b0.y * av.x + b1.y * av.y) + b2.y * av.z;
    let tz = (av.z * b2.z) + (av.x * b0.z + av.y * b1.z);
    let t3 = Vec3::new(tx + 0.5 * correction.x, ty + 0.5 * correction.y, 0.5 * correction.z + tz);
    set_joint(h, bodies, end, t3, p.spin_limit[2], p.pose_spin[2]);
}
