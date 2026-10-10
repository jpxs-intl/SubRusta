use glam::Vec3;

use crate::{
    RigidBody, Table,
    body::RigidBodyType,
    body::{BodyContact, Contact, GroundContact},
};

const JOINT_STIFFNESS: f32 = 0.1875;
const JOINT_MAX_ERROR: f32 = 0.0625;
const JOINT_MAX_IMPULSE: f32 = 1.0 / 30.0;
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
            Bond::Joint(_) => 65536,
            Bond::ItemPoint(b) if b.permanent => 65536,
            Bond::ItemAngular(b) if b.permanent => 65536,
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
    /// A vehicle's own bond (vehicle_bond_something), kept from tick to tick.
    pub permanent: bool,
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
        Self { body_a, body_b, anchor_a, anchor_b, rest: 0.0, permanent: false, solve: PointSolve::default() }
    }

    /// vehicle_bond_something: a lasting point bond between two bodies of one vehicle.
    pub fn vehicle(body_a: usize, body_b: usize, anchor_a: Vec3, anchor_b: Vec3) -> Self {
        Self { permanent: true, ..Self::new(body_a, body_b, anchor_a, anchor_b) }
    }

    pub(crate) fn prepare(&mut self, bodies: &Table<RigidBody>) {
        let (Some(a), Some(b)) = (bodies.get(self.body_a), bodies.get(self.body_b)) else { return };
        let ra = local_to_world(a, self.anchor_a);
        let rb = local_to_world(b, self.anchor_b);
        let d = a.pos + ra - (rb + b.pos);
        let n = d.normalize_or_zero();
        let c = -self.rest;
        let mut corr = n * c + d;
        let both_vehicles = matches!(a.kind, RigidBodyType::Vehicle) && matches!(b.kind, RigidBodyType::Vehicle);
        let (k, stiffness) = if both_vehicles { (1.0 / 256.0, 0.125) } else { (0.1875, 0.1875) };
        if !both_vehicles && matches!(b.kind, RigidBodyType::Item) {
            let len = corr.length();
            if len > 0.125 {
                let f = 0.125 / len;
                corr *= f;
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
            a.impulse = nwa * u + a.impulse;
            let l = a.ang_impulse;
            a.ang_impulse = u.cross(ra) * nwa + l;
        }
        if let Some(b) = bodies.get_mut(self.body_b) {
            b.impulse = wb_ * u + b.impulse;
            let l = b.ang_impulse;
            b.ang_impulse = u.cross(rb) * wb_ + l;
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
    /// Bond data +0x5c and +0x60: the axis the turn is kept off (the train's bogies turn only about it).
    pub axis_lock: Option<Vec3>,
    pub permanent: bool,
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
        Self { body_a, body_b, target, spin_limit: 4.0 / 15.0, axis_lock: None, permanent: false, solve: AngularSolve::default() }
    }

    /// create_bond_type8: a lasting angular bond between two bodies of one vehicle, with no spin limit.
    pub fn vehicle(body_a: usize, body_b: usize) -> Self {
        Self { spin_limit: 0.0, permanent: true, ..Self::new(body_a, body_b, Vec3::ZERO) }
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
        self.solve = AngularSolve {
            target: Vec3::new(t.x * JOINT_DAMPING, t.y * JOINT_DAMPING, t.z * JOINT_DAMPING),
            weight_a: weight_a * cap,
            weight_b: weight_b * cap,
        };
    }

    pub(crate) fn solve(&self, bodies: &mut Table<RigidBody>) {
        let s = &self.solve;
        let (Some(a), Some(b)) = (bodies.get(self.body_a), bodies.get(self.body_b)) else { return };
        let (wa, wb) = (a.ang_vel, b.ang_vel);
        let mut t = JOINT_DAMPING * wb + (-JOINT_DAMPING * wa + s.target);
        if let Some(ax) = self.axis_lock {
            let d = -t.dot(ax);
            t += ax * d;
        }
        if self.spin_limit != 0.0 {
            let len = t.length();
            if len > self.spin_limit {
                let f = self.spin_limit / len;
                t *= f;
            }
            t += (wb - wa) * 0.125;
        }
        let (ka, nkb) = (s.weight_a, -s.weight_b);
        if let Some(a) = bodies.get_mut(self.body_a) {
            let l = a.ang_impulse;
            a.ang_impulse = ka * t + l;
        }
        if let Some(b) = bodies.get_mut(self.body_b) {
            let l = b.ang_impulse;
            b.ang_impulse = l + t * nkb;
        }
    }
}

/// The linear impulse a point bond asks for: the error plus the relative velocity of the two anchor points, capped.
#[allow(clippy::too_many_arguments)]
fn point_impulse(bodies: &Table<RigidBody>, body_a: usize, body_b: usize, ra: Vec3, rb: Vec3, error: Vec3, k: f32) -> Option<Vec3> {
    let (a, b) = (bodies.get(body_a)?, bodies.get(body_b)?);
    let (va, wa, vb, wb) = (a.vel, a.ang_vel, b.vel, b.ang_vel);
    let nk = -k;
    let pa = ra.cross(wa) + va;
    let pb = rb.cross(wb) + vb;
    let u = pa * k + error + pb * nk;
    let len = u.length();
    Some(if len > JOINT_MAX_IMPULSE {
        let f = JOINT_MAX_IMPULSE / len;
        u * f
    } else {
        u
    })
}

#[derive(Clone, Debug)]
pub struct Joint {
    pub body_a: usize,
    pub body_b: usize,
    pub anchor_a: Vec3,
    pub anchor_b: Vec3,
    pub owner: i32,
    pub spin_limit: f32,
    pub spin_damping: f32,
    pub target_ang_vel: Vec3,
    pub limit_correction: Vec3,
    pub limit_active: bool,
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
        Self {
            body_a,
            body_b,
            anchor_a,
            anchor_b,
            owner,
            spin_limit: 0.0,
            spin_damping: 0.0,
            target_ang_vel: Vec3::ZERO,
            limit_correction: Vec3::ZERO,
            limit_active: false,
            solve: JointSolve::default(),
        }
    }

    pub(crate) fn prepare(&mut self, bodies: &Table<RigidBody>) {
        let (Some(a), Some(b)) = (bodies.get(self.body_a), bodies.get(self.body_b)) else { return };
        let ra = local_to_world(a, self.anchor_a);
        let rb = local_to_world(b, self.anchor_b);
        let mut d = a.pos + ra - (rb + b.pos);
        let len = d.length();
        if len > JOINT_MAX_ERROR {
            let inv = 1.0 / len;
            d = d * inv * JOINT_MAX_ERROR;
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

        let pa = ra.cross(wa) + va;
        let pb = rb.cross(wb) + vb;
        let mut u = pa * k + s.error + pb * nk;
        let len = u.length();
        if len > JOINT_MAX_IMPULSE {
            let f = JOINT_MAX_IMPULSE / len;
            u *= f;
        }

        let mut t = wa * -JOINT_DAMPING + s.target + wb * JOINT_DAMPING;
        if self.spin_limit != 0.0 {
            let len = t.length();
            if len > self.spin_limit {
                let f = self.spin_limit / len;
                t *= f;
            }
            let d = wb - wa;
            let k = if self.spin_damping == 0.0 { 0.125 } else { self.spin_damping };
            t += d * k;
        }
        if self.limit_active {
            let c = self.limit_correction;
            let d = wb - wa;
            let u = c.normalize_or_zero();
            let proj = d.dot(u);
            let add = c * JOINT_DAMPING + u * proj * JOINT_DAMPING;
            t += add;
        }
        t *= s.inertia_cap;

        let (wa_, wb_) = (s.weight_a, s.weight_b);
        let (nwa, nwb) = (-wa_, -wb_);
        if let Some(a) = bodies.get_mut(self.body_a) {
            a.impulse = nwa * u + a.impulse;
            let mut l = a.ang_impulse;
            l.x += (ra.z * u.y - u.z * ra.y) * nwa;
            l.y += (u.z * ra.x - ra.z * u.x) * nwa;
            l.z += (ra.y * u.x - ra.x * u.y) * nwa;
            l = wa_ * t + l;
            a.ang_impulse = l;
        }
        if let Some(b) = bodies.get_mut(self.body_b) {
            b.impulse = wb_ * u + b.impulse;
            let mut l = b.ang_impulse;
            l.x += (rb.z * u.y - u.z * rb.y) * wb_;
            l.y += (u.z * rb.x - rb.z * u.x) * wb_;
            l.z += (u.x * rb.y - u.y * rb.x) * wb_;
            l = t * nwb + l;
            b.ang_impulse = l;
        }
    }
}

fn local_to_world(b: &RigidBody, l: Vec3) -> Vec3 {
    let [r0, r1, r2] = b.rot;
    r0 * l.x + r1 * l.y + r2 * l.z
}
