use glam::Vec3;

use crate::{
    RigidBody, Table,
    body::RigidBodyType,
    body::{BodyContact, Contact, GroundContact},
};

pub const NEVER_DESPAWN: i32 = 65536;

const JOINT_STIFFNESS: f32 = 0.1875;
const JOINT_MAX_ERROR: f32 = 0.0625;
const JOINT_MAX_IMPULSE: f32 = f32::from_bits(0x3d08_8889);
const JOINT_DAMPING: f32 = 0.875;

#[derive(Clone, Debug)]
pub enum Bond {
    Joint(Joint),
    WorldContact(Contact),
    BodyContact(BodyContact),
    GroundContact(GroundContact),
    ItemPoint(ItemPoint),
    ItemAngular(ItemAngular),
}

impl Bond {
    pub fn despawn_time(&self) -> i32 {
        match self {
            Bond::Joint(_) => NEVER_DESPAWN,
            Bond::WorldContact(_) | Bond::BodyContact(_) | Bond::GroundContact(_) | Bond::ItemPoint(_) | Bond::ItemAngular(_) => 0,
        }
    }
}

/// Bond type 7: pulls a point on `body_b` (a held item) onto a point on `body_a` (the hand).
#[derive(Clone, Debug)]
pub struct ItemPoint {
    pub body_a: usize,
    pub body_b: usize,
    pub anchor_a: Vec3,
    pub anchor_b: Vec3,
    pub rest: f32,
    solve: PointSolve,
}

#[derive(Clone, Debug, Default)]
struct PointSolve {
    ra: Vec3,
    rb: Vec3,
    error: Vec3,
    stiffness: f32,
    weight_a: f32,
    weight_b: f32,
}

impl ItemPoint {
    pub fn new(body_a: usize, body_b: usize, anchor_a: Vec3, anchor_b: Vec3) -> Self {
        Self { body_a, body_b, anchor_a, anchor_b, rest: 0.0, solve: PointSolve::default() }
    }

    pub(crate) fn prepare(&mut self, bodies: &Table<RigidBody>) {
        let (Some(a), Some(b)) = (bodies.get(self.body_a), bodies.get(self.body_b)) else { return };
        let ra = local_to_world(a, self.anchor_a);
        let rb = local_to_world(b, self.anchor_b);
        let d = Vec3::new((a.pos.x + ra.x) - (rb.x + b.pos.x), (a.pos.y + ra.y) - (rb.y + b.pos.y), (ra.z + a.pos.z) - (rb.z + b.pos.z));
        let len = ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt();
        let n = if len == 0.0 {
            Vec3::ZERO
        } else {
            let inv = 1.0 / len;
            Vec3::new(d.x * inv, d.y * inv, inv * d.z)
        };
        let c = -self.rest;
        let mut corr = Vec3::new(n.x * c + d.x, d.y + n.y * c, c * n.z + d.z);
        let both_vehicles = matches!(a.kind, RigidBodyType::Vehicle) && matches!(b.kind, RigidBodyType::Vehicle);
        let (k, stiffness) = if both_vehicles { (0.00390625, 0.125) } else { (0.1875, 0.1875) };
        if !both_vehicles && matches!(b.kind, RigidBodyType::Item) {
            let len = ((corr.x * corr.x + corr.y * corr.y) + corr.z * corr.z).sqrt();
            if len > 0.125 {
                let f = 0.125 / len;
                corr = Vec3::new(corr.x * f, corr.y * f, corr.z * f);
            }
        }
        let inv = 1.0 / (a.mass + b.mass);
        self.solve = PointSolve { ra, rb, error: Vec3::new(corr.x * k, corr.y * k, corr.z * k), stiffness, weight_a: b.mass * inv, weight_b: a.mass * inv };
    }

    pub(crate) fn solve(&self, bodies: &mut Table<RigidBody>) {
        let s = &self.solve;
        let Some(u) = point_impulse(bodies, self.body_a, self.body_b, s.ra, s.rb, s.error, s.stiffness) else { return };
        let (ra, rb) = (s.ra, s.rb);
        let (wb_, nwa) = (s.weight_b, -s.weight_a);
        if let Some(a) = bodies.get_mut(self.body_a) {
            a.impulse = Vec3::new(nwa * u.x + a.impulse.x, nwa * u.y + a.impulse.y, nwa * u.z + a.impulse.z);
            let l = a.ang_impulse;
            a.ang_impulse = Vec3::new((ra.z * u.y - u.z * ra.y) * nwa + l.x, (u.z * ra.x - ra.z * u.x) * nwa + l.y, (ra.y * u.x - ra.x * u.y) * nwa + l.z);
        }
        if let Some(b) = bodies.get_mut(self.body_b) {
            b.impulse = Vec3::new(wb_ * u.x + b.impulse.x, wb_ * u.y + b.impulse.y, wb_ * u.z + b.impulse.z);
            let l = b.ang_impulse;
            b.ang_impulse = Vec3::new((rb.z * u.y - u.z * rb.y) * wb_ + l.x, (u.z * rb.x - rb.z * u.x) * wb_ + l.y, (u.x * rb.y - u.y * rb.x) * wb_ + l.z);
        }
    }
}

/// Bond type 8: turns `body_b` (a held item) towards the orientation it should have in `body_a` (the hand).
#[derive(Clone, Debug)]
pub struct ItemAngular {
    pub body_a: usize,
    pub body_b: usize,
    pub target: Vec3,
    pub spin_limit: f32,
    solve: AngularSolve,
}

#[derive(Clone, Debug, Default)]
struct AngularSolve {
    target: Vec3,
    weight_a: f32,
    weight_b: f32,
}

impl ItemAngular {
    pub fn new(body_a: usize, body_b: usize, target: Vec3) -> Self {
        Self { body_a, body_b, target, spin_limit: f32::from_bits(0x3e88_8889), solve: AngularSolve::default() }
    }

    pub(crate) fn prepare(&mut self, bodies: &Table<RigidBody>) {
        let (Some(a), Some(b)) = (bodies.get(self.body_a), bodies.get(self.body_b)) else { return };
        let inv = 1.0 / (a.mass + b.mass);
        let (weight_a, weight_b) = (b.mass * inv, a.mass * inv);
        let cap_b = b.min_inertia * weight_b;
        let cap_a = a.min_inertia * weight_a;
        let (cap_a, cap_b) = (cap_a + cap_a, cap_b + cap_b);
        let cap = if cap_a < cap_b { cap_a } else { cap_b };
        let t = self.target;
        self.solve = AngularSolve { target: Vec3::new(t.x * JOINT_DAMPING, t.y * JOINT_DAMPING, t.z * JOINT_DAMPING), weight_a: weight_a * cap, weight_b: weight_b * cap };
    }

    pub(crate) fn solve(&self, bodies: &mut Table<RigidBody>) {
        let s = &self.solve;
        let (Some(a), Some(b)) = (bodies.get(self.body_a), bodies.get(self.body_b)) else { return };
        let (wa, wb) = (a.ang_vel, b.ang_vel);
        // TODO: when the global byte at 0x5b08aec0 is set the binary takes another path here (0x462111)
        let mut t = Vec3::new(
            JOINT_DAMPING * wb.x + (-JOINT_DAMPING * wa.x + s.target.x),
            JOINT_DAMPING * wb.y + (-JOINT_DAMPING * wa.y + s.target.y),
            JOINT_DAMPING * wb.z + (-JOINT_DAMPING * wa.z + s.target.z),
        );
        if self.spin_limit != 0.0 {
            let len = ((t.x * t.x + t.y * t.y) + t.z * t.z).sqrt();
            if len > self.spin_limit {
                let f = self.spin_limit / len;
                t = Vec3::new(t.x * f, t.y * f, t.z * f);
            }
            t = Vec3::new(t.x + (wb.x - wa.x) * 0.125, t.y + (wb.y - wa.y) * 0.125, t.z + (wb.z - wa.z) * 0.125);
        }
        let (ka, nkb) = (s.weight_a, -s.weight_b);
        if let Some(a) = bodies.get_mut(self.body_a) {
            let l = a.ang_impulse;
            a.ang_impulse = Vec3::new(ka * t.x + l.x, ka * t.y + l.y, ka * t.z + l.z);
        }
        if let Some(b) = bodies.get_mut(self.body_b) {
            let l = b.ang_impulse;
            b.ang_impulse = Vec3::new(l.x + t.x * nkb, l.y + t.y * nkb, l.z + t.z * nkb);
        }
    }
}

/// The linear impulse a point bond asks for: the error plus the relative velocity of the two anchor points, capped.
#[allow(clippy::too_many_arguments)]
fn point_impulse(bodies: &Table<RigidBody>, body_a: usize, body_b: usize, ra: Vec3, rb: Vec3, error: Vec3, k: f32) -> Option<Vec3> {
    let (a, b) = (bodies.get(body_a)?, bodies.get(body_b)?);
    let (va, wa, vb, wb) = (a.vel, a.ang_vel, b.vel, b.ang_vel);
    let nk = -k;
    let pa = Vec3::new((wa.z * ra.y - wa.y * ra.z) + va.x, (wa.x * ra.z - wa.z * ra.x) + va.y, (wa.y * ra.x - wa.x * ra.y) + va.z);
    let pb = Vec3::new((wb.z * rb.y - wb.y * rb.z) + vb.x, (wb.x * rb.z - wb.z * rb.x) + vb.y, (wb.y * rb.x - wb.x * rb.y) + vb.z);
    let u = Vec3::new(((pa.x * k) + error.x) + pb.x * nk, ((pa.y * k) + error.y) + pb.y * nk, (nk * pb.z) + ((k * pa.z) + error.z));
    let len = ((u.x * u.x + u.y * u.y) + u.z * u.z).sqrt();
    Some(if len > JOINT_MAX_IMPULSE {
        let f = JOINT_MAX_IMPULSE / len;
        Vec3::new(u.x * f, u.y * f, u.z * f)
    } else {
        u
    })
}

// TODO: name `params` once its readers are ported
#[derive(Clone, Debug)]
pub struct Joint {
    pub body_a: usize,
    pub body_b: usize,
    pub anchor_a: Vec3,
    pub anchor_b: Vec3,
    pub params: [f32; 2],
    pub owner: i32,
    pub spin_limit: f32,
    pub spin_damping: f32,
    pub target_ang_vel: Vec3,
    pub limit_correction: Vec3,
    pub limit_active: bool,
    // TODO: written by the pose IK (bond +0x8c and +0x78) but not read by the joint solver; find their readers
    pub pose_spin: f32,
    pub pose_limit: Vec3,
    solve: JointSolve,
}

#[derive(Clone, Debug, Default)]
struct JointSolve {
    ra: Vec3,
    rb: Vec3,
    error: Vec3,
    weight_a: f32,
    weight_b: f32,
    target: Vec3,
    inertia_cap: f32,
}

impl Joint {
    pub fn new(body_a: usize, body_b: usize, anchor_a: Vec3, anchor_b: Vec3, owner: i32) -> Self {
        Self { body_a, body_b, anchor_a, anchor_b, params: [0.25, 0.375], owner, spin_limit: 0.0, spin_damping: 0.0, target_ang_vel: Vec3::ZERO, limit_correction: Vec3::ZERO, limit_active: false, pose_spin: 0.0, pose_limit: Vec3::ZERO, solve: JointSolve::default() }
    }

    pub(crate) fn prepare(&mut self, bodies: &Table<RigidBody>) {
        let (Some(a), Some(b)) = (bodies.get(self.body_a), bodies.get(self.body_b)) else { return };
        let ra = local_to_world(a, self.anchor_a);
        let rb = local_to_world(b, self.anchor_b);
        let mut d = Vec3::new((a.pos.x + ra.x) - (rb.x + b.pos.x), (a.pos.y + ra.y) - (rb.y + b.pos.y), (a.pos.z + ra.z) - (rb.z + b.pos.z));
        let len = ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt();
        if len > JOINT_MAX_ERROR {
            let inv = 1.0 / len;
            d = Vec3::new(d.x * inv * JOINT_MAX_ERROR, d.y * inv * JOINT_MAX_ERROR, d.z * inv * JOINT_MAX_ERROR);
        }
        let inv_mass = 1.0 / (a.mass + b.mass);
        let (weight_a, weight_b) = (b.mass * inv_mass, a.mass * inv_mass);
        let cap_a = a.min_inertia * weight_a;
        let cap_b = b.min_inertia * weight_b;
        let (cap_a, cap_b) = (cap_a + cap_a, cap_b + cap_b);
        let t = self.target_ang_vel;
        self.solve = JointSolve {
            ra,
            rb,
            error: d * JOINT_STIFFNESS,
            weight_a,
            weight_b,
            target: Vec3::new(t.x * JOINT_DAMPING, t.y * JOINT_DAMPING, t.z * JOINT_DAMPING),
            inertia_cap: if cap_a < cap_b { cap_a } else { cap_b },
        };
    }

    pub(crate) fn solve(&self, bodies: &mut Table<RigidBody>) {
        let s = &self.solve;
        let (Some(a), Some(b)) = (bodies.get(self.body_a), bodies.get(self.body_b)) else { return };
        let (va, wa, vb, wb) = (a.vel, a.ang_vel, b.vel, b.ang_vel);
        let (ra, rb, k, nk) = (s.ra, s.rb, JOINT_STIFFNESS, -JOINT_STIFFNESS);

        let pa = Vec3::new((wa.z * ra.y - wa.y * ra.z) + va.x, (wa.x * ra.z - wa.z * ra.x) + va.y, (wa.y * ra.x - wa.x * ra.y) + va.z);
        let pb = Vec3::new((wb.z * rb.y - wb.y * rb.z) + vb.x, (wb.x * rb.z - wb.z * rb.x) + vb.y, (wb.y * rb.x - wb.x * rb.y) + vb.z);
        let mut u = Vec3::new(
            ((pa.x * k) + s.error.x) + pb.x * nk,
            ((pa.y * k) + s.error.y) + pb.y * nk,
            (nk * pb.z) + ((k * pa.z) + s.error.z),
        );
        let len = ((u.x * u.x + u.y * u.y) + u.z * u.z).sqrt();
        if len > JOINT_MAX_IMPULSE {
            let f = JOINT_MAX_IMPULSE / len;
            u = Vec3::new(u.x * f, u.y * f, u.z * f);
        }

        let mut t = Vec3::new(
            ((wa.x * -JOINT_DAMPING) + s.target.x) + wb.x * JOINT_DAMPING,
            ((wa.y * -JOINT_DAMPING) + s.target.y) + wb.y * JOINT_DAMPING,
            ((-JOINT_DAMPING * wa.z) + s.target.z) + wb.z * JOINT_DAMPING,
        );
        if self.spin_limit != 0.0 {
            let len = ((t.x * t.x + t.y * t.y) + t.z * t.z).sqrt();
            if len > self.spin_limit {
                let f = self.spin_limit / len;
                t = Vec3::new(t.x * f, t.y * f, t.z * f);
            }
            let d = Vec3::new(wb.x - wa.x, wb.y - wa.y, wb.z - wa.z);
            let k = if self.spin_damping == 0.0 { 0.125 } else { self.spin_damping };
            t = Vec3::new(t.x + d.x * k, t.y + d.y * k, t.z + k * d.z);
        }
        if self.limit_active {
            let c = self.limit_correction;
            let d = Vec3::new(wb.x - wa.x, wb.y - wa.y, wb.z - wa.z);
            let len = ((c.x * c.x + c.y * c.y) + c.z * c.z).sqrt();
            let u = if len == 0.0 {
                Vec3::ZERO
            } else {
                let inv = 1.0 / len;
                Vec3::new(inv * c.x, inv * c.y, inv * c.z)
            };
            let proj = d.z * u.z + (d.y * u.y + d.x * u.x);
            let add = Vec3::new(
                c.x * JOINT_DAMPING + u.x * proj * JOINT_DAMPING,
                c.y * JOINT_DAMPING + u.y * proj * JOINT_DAMPING,
                JOINT_DAMPING * c.z + u.z * proj * JOINT_DAMPING,
            );
            t = Vec3::new(t.x + add.x, t.y + add.y, t.z + add.z);
        }
        t = Vec3::new(t.x * s.inertia_cap, t.y * s.inertia_cap, t.z * s.inertia_cap);

        let (wa_, wb_) = (s.weight_a, s.weight_b);
        let (nwa, nwb) = (-wa_, -wb_);
        if let Some(a) = bodies.get_mut(self.body_a) {
            a.impulse = Vec3::new(nwa * u.x + a.impulse.x, nwa * u.y + a.impulse.y, nwa * u.z + a.impulse.z);
            let mut l = a.ang_impulse;
            l.x += (ra.z * u.y - u.z * ra.y) * nwa;
            l.y += (u.z * ra.x - ra.z * u.x) * nwa;
            l.z += (ra.y * u.x - ra.x * u.y) * nwa;
            l = Vec3::new(wa_ * t.x + l.x, wa_ * t.y + l.y, wa_ * t.z + l.z);
            a.ang_impulse = l;
        }
        if let Some(b) = bodies.get_mut(self.body_b) {
            b.impulse = Vec3::new(wb_ * u.x + b.impulse.x, wb_ * u.y + b.impulse.y, wb_ * u.z + b.impulse.z);
            let mut l = b.ang_impulse;
            l.x += (rb.z * u.y - u.z * rb.y) * wb_;
            l.y += (u.z * rb.x - rb.z * u.x) * wb_;
            l.z += (u.x * rb.y - u.y * rb.x) * wb_;
            l = Vec3::new(t.x * nwb + l.x, t.y * nwb + l.y, nwb * t.z + l.z);
            b.ang_impulse = l;
        }
    }
}

fn local_to_world(b: &RigidBody, l: Vec3) -> Vec3 {
    let [r0, r1, r2] = b.rot;
    Vec3::new((r0.x * l.x + r1.x * l.y) + r2.x * l.z, (r0.y * l.x + r1.y * l.y) + r2.y * l.z, (r0.z * l.x + r1.z * l.y) + r2.z * l.z)
}
