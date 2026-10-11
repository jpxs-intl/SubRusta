use glam::Vec3;

use crate::{
    Bond, Table,
    rotation::{RotMatrix, angular_velocity, rk4_rotation},
};

#[derive(Clone, Debug)]
#[repr(i32)]
pub enum RigidBodyType {
    HumanBone = 0,
    Vehicle = 1,
    Wheel = 2,
    Item = 3,
}

#[derive(Clone, Debug)]
pub struct RigidBody {
    pub kind: RigidBodyType,
    pub settled: bool,
    pub owner: i32,
    pub part: i32,
    pub mass: f32,
    pub pos: Vec3,
    pub vel: Vec3,
    pub impulse: Vec3,
    pub rot: RotMatrix,
    pub ang_momentum: Vec3,
    pub ang_vel: Vec3,
    pub ang_impulse: Vec3,
    pub inertia: Vec3,
    pub inv_inertia: Vec3,
    pub min_inertia: f32,
    pub coefs: [f32; 3],
    pub slide: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub body: usize,
    pub offset: Vec3,
    pub normal: Vec3,
    pub bias: f32,
    pub softness: f32,
    pub friction: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct BodyContact {
    pub body_a: usize,
    pub body_b: usize,
    pub offset_a: Vec3,
    pub offset_b: Vec3,
    pub normal: Vec3,
    pub bias: f32,
    pub softness: f32,
    pub friction: f32,
    pub weight_a: f32,
    pub weight_b: f32,
}

/// A foot held against the ground (bond type 9): pushes the body along `normal` and steers its spin towards `spin`.
#[derive(Clone, Copy, Debug)]
pub struct GroundContact {
    pub body: usize,
    pub depth: f32,
    pub stiffness: f32,
    pub damping: f32,
    pub normal: Vec3,
    pub friction: f32,
    pub drift: Vec3,
    pub ground_vel: Vec3,
    pub spin: Vec3,
    pub spin_gain: f32,
    pub lift: f32,
}

/// Where a solver iteration is when `RigidBodies::solve_bonds_with` calls its step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolverStep {
    BeforeBonds,
    AfterBonds,
}

/// How much a free body's vertical speed drops each tick: 9.8 / 60².
pub const GRAVITY: f32 = 9.8 / (60.0 * 60.0);

pub struct RigidBodies {
    bodies: Table<RigidBody>,
    bonds: Table<Bond>,
    /// A multiplier on gravity (1 as in the game), changed by the /gravity command.
    pub gravity_scale: f32,
}

impl Default for RigidBodies {
    fn default() -> Self {
        Self { bodies: Table::unbounded(), bonds: Table::unbounded(), gravity_scale: 1.0 }
    }
}

impl RigidBodies {
    pub fn create(&mut self, kind: RigidBodyType, pos: Vec3, rot: RotMatrix, vel: Option<Vec3>, inertia: Vec3, mass: f32) -> Option<usize> {
        let inv_inertia = Vec3::from_array(inertia.to_array().map(|i| if i <= 0.0 { 0.0 } else { 1.0 / i }));
        let mut min_inertia = inertia.x;
        if min_inertia > inertia.y {
            min_inertia = inertia.y;
        }
        if !(min_inertia <= inertia.z) {
            min_inertia = inertia.z;
        }
        self.bodies.insert(RigidBody {
            kind,
            settled: false,
            owner: 0,
            part: 0,
            mass,
            pos,
            vel: vel.unwrap_or(Vec3::ZERO),
            impulse: Vec3::ZERO,
            rot,
            ang_momentum: Vec3::ZERO,
            ang_vel: Vec3::ZERO,
            ang_impulse: Vec3::ZERO,
            inertia,
            inv_inertia,
            min_inertia,
            coefs: [0.5, 0.25, 0.5],
            slide: 0.0,
        })
    }

    pub fn remove(&mut self, id: usize) -> Option<RigidBody> {
        self.bodies.remove(id)
    }

    pub fn get(&self, id: usize) -> Option<&RigidBody> {
        self.bodies.get(id)
    }

    pub fn get_mut(&mut self, id: usize) -> Option<&mut RigidBody> {
        self.bodies.get_mut(id)
    }

    pub fn create_bond(&mut self, bond: Bond) -> Option<usize> {
        self.bonds.insert(bond)
    }

    pub fn remove_bond(&mut self, id: usize) -> Option<Bond> {
        self.bonds.remove(id)
    }

    pub fn bond(&self, id: usize) -> Option<&Bond> {
        self.bonds.get(id)
    }

    pub fn bond_mut(&mut self, id: usize) -> Option<&mut Bond> {
        self.bonds.get_mut(id)
    }

    pub fn simulate(&mut self) {
        let gravity = GRAVITY * self.gravity_scale;

        // self.bodies.par_iter_mut().for_each(|(_, b)| {
        //     if b.settled {
        //         return;
        //     }
        //
        //     let vy = b.vel.y - gravity;
        //     b.vel.y = vy;
        //     b.pos.x += b.vel.x;
        //     b.pos.y += vy;
        //     b.pos.z += b.vel.z;
        //     b.ang_vel = rk4_rotation(&mut b.rot, b.ang_momentum, b.inv_inertia);
        // });

        for (_, b) in self.bodies.iter_mut() {
            if b.settled {
                continue;
            }

            let vy = b.vel.y - gravity;
            b.vel.y = vy;
            b.pos.x += b.vel.x;
            b.pos.y += vy;
            b.pos.z += b.vel.z;
            b.ang_vel = rk4_rotation(&mut b.rot, b.ang_momentum, b.inv_inertia);
        }
    }

    pub fn add_impulse(&mut self, id: usize, offset: Vec3, impulse: Vec3) {
        let Some(b) = self.bodies.get_mut(id) else { return };
        let (r, j) = (offset, impulse);
        b.vel += j;
        b.ang_momentum = j.cross(r) + b.ang_momentum;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_world_contact(&mut self, body: usize, offset: Vec3, normal: Vec3, depth: f32, friction: f32, depth_scale: f32, softness: f32) {
        self.bonds.insert(Bond::WorldContact(Contact { body, offset, normal, bias: depth * depth_scale, softness, friction }));
    }

    /// add_collision_rigidbody_on_rigidbody: a one-tick contact pushing `body_a` along `normal` and `body_b` against it.
    #[allow(clippy::too_many_arguments)]
    pub fn add_body_contact(
        &mut self,
        body_a: usize,
        body_b: usize,
        offset_a: Vec3,
        offset_b: Vec3,
        normal: Vec3,
        depth: f32,
        friction: f32,
        depth_scale: f32,
        softness: f32,
    ) {
        let (Some(a), Some(b)) = (self.bodies.get(body_a), self.bodies.get(body_b)) else { return };
        let inv = 1.0 / (a.mass + b.mass);
        let (weight_a, weight_b) = (b.mass * inv, a.mass * inv);
        self.bonds.insert(Bond::BodyContact(BodyContact {
            body_a,
            body_b,
            offset_a,
            offset_b,
            normal,
            bias: depth * depth_scale,
            softness,
            friction,
            weight_a,
            weight_b,
        }));
    }

    /// How many bonds and contacts are in the table.
    pub fn bond_count(&self) -> usize {
        self.bonds.iter().count()
    }

    pub fn solve_bonds(&mut self) {
        self.solve_bonds_with(|_, _| {});
    }

    /// bond_simulation: the bonds solved 32 times, `step` running on the bodies before (vehicles_step_wheel_constraints)
    /// and after (apply_vehicle_wheel_forces) each pass over the bonds.
    pub fn solve_bonds_with(&mut self, mut step: impl FnMut(&mut Table<RigidBody>, SolverStep)) {
        let Self { bodies, bonds, .. } = self;

        for (_, bond) in bonds.iter_mut() {
            match bond {
                Bond::Joint(j) => j.prepare(bodies),
                Bond::ItemPoint(j) => j.prepare(bodies),
                Bond::ItemAngular(j) => j.prepare(bodies),
                _ => {}
            }
        }

        for (_, b) in bodies.iter_mut() {
            b.slide = 0.0;
        }

        for _ in 0..32 {
            for (_, b) in bodies.iter_mut() {
                b.impulse = Vec3::ZERO;
                b.ang_impulse = Vec3::ZERO;
            }

            step(bodies, SolverStep::BeforeBonds);

            for (_, bond) in bonds.iter() {
                match bond {
                    Bond::Joint(j) => j.solve(bodies),
                    Bond::ItemPoint(j) => j.solve(bodies),
                    Bond::ItemAngular(j) => j.solve(bodies),
                    Bond::WorldContact(c) => {
                        if let Some(b) = bodies.get_mut(c.body) {
                            solve_world_contact(b, c);
                        }
                    }
                    Bond::BodyContact(c) => solve_body_contact(bodies, c),
                    Bond::GroundContact(c) => {
                        if let Some(b) = bodies.get_mut(c.body) {
                            solve_ground_contact(b, c);
                        }
                    }
                }
            }

            step(bodies, SolverStep::AfterBonds);

            for (_, b) in bodies.iter_mut() {
                if !b.settled {
                    apply_impulses(b);
                }
            }
        }

        for id in bonds.ids() {
            if bonds.get(id).is_some_and(|b| b.despawn_time() == 0) {
                bonds.remove(id);
            }
        }
    }
}

fn solve_world_contact(b: &mut RigidBody, c: &Contact) {
    let (r, n, w, v) = (c.offset, c.normal, b.ang_vel, b.vel);
    let k = -c.softness;
    let u = (r.cross(w) + v) * k + c.bias * n;
    let d = u.dot(n);
    if !(d > 0.0) {
        return;
    }
    let mut j = n * d;
    let t = u - j;
    let tlen = t.length();
    let mut slide = 0.0f32;
    if tlen > 0.0 {
        let inv = 1.0 / tlen;
        let d = t * inv;
        let jlen = j.length();
        let limit = c.friction * jlen;
        let k = 24.5 / 3600.0;
        let m = if tlen > limit {
            slide = (((tlen - limit) * jlen) * 60.0) * 8.0 + 0.0;
            if limit > k {
                slide += ((jlen * (limit - k)) * 60.0) * 8.0;
                k
            } else {
                limit
            }
        } else if tlen > k {
            slide = 0.0 + ((jlen * (tlen - k)) * 60.0) * 8.0;
            k
        } else {
            tlen
        };
        j.x += d.x * m;
        j.y += d.y * m;
        j.z += m * d.z;
    }
    b.slide += slide;
    b.impulse += j;
    b.ang_impulse = j.cross(r) + b.ang_impulse;
}

fn solve_body_contact(bodies: &mut Table<RigidBody>, c: &BodyContact) {
    let (Some(a), Some(b)) = (bodies.get(c.body_a), bodies.get(c.body_b)) else { return };
    let (va, wa, vb, wb) = (a.vel, a.ang_vel, b.vel, b.ang_vel);
    let (ra, rb, n, s, bias) = (c.offset_a, c.offset_b, c.normal, c.softness, c.bias);
    let ns = -s;
    let u = (va + ra.cross(wa)) * ns + bias * n + (rb.cross(wb) + vb) * s;
    let d = u.dot(n);
    if !(d > 0.0) {
        return;
    }
    let mut j = n * d;
    let t = u - j;
    let tlen = t.length();
    if tlen > 0.0 {
        let inv = 1.0 / tlen;
        let d = t * inv;
        let jlen = j.length();
        let limit = c.friction * jlen;
        let m = if limit < tlen { limit } else { tlen };
        j.x += d.x * m;
        j.y += d.y * m;
        j.z += m * d.z;
    }

    let (wa_, nwb) = (c.weight_a, -c.weight_b);
    if let Some(a) = bodies.get_mut(c.body_a) {
        a.impulse = wa_ * j + a.impulse;
        let l = a.ang_impulse;
        a.ang_impulse = j.cross(ra) * wa_ + l;
    }
    if let Some(b) = bodies.get_mut(c.body_b) {
        b.impulse = nwb * j + b.impulse;
        let l = b.ang_impulse;
        b.ang_impulse = j.cross(rb) * nwb + l;
    }
}

fn solve_ground_contact(b: &mut RigidBody, c: &GroundContact) {
    let (n, v, g) = (c.normal, b.vel, c.ground_vel);
    let push = c.depth * c.stiffness;
    let k = c.stiffness * 0.125;
    let nd = -c.damping;
    let u = (v + 0.0 - g) * nd + (c.drift * k + push * n);
    let d = u.dot(n);
    if !(d > 0.0) {
        return;
    }
    let mut j = n * d;
    let t = u - j;
    let tl = t.length();
    if tl > 0.0 {
        let inv = 1.0 / tl;
        let d = t * inv;
        let jl = j.length();
        let limit = c.friction * jl;
        let m = if limit < tl { limit } else { tl };
        j.x += d.x * m;
        j.y += d.y * m;
        j.z += d.z * m;
    }
    let w = d * 1024.0;
    let w = if 1.0 < w { 1.0 } else { w };
    b.impulse = Vec3::new(b.impulse.x + j.x, c.lift * j.y + b.impulse.y, j.z + b.impulse.z);
    let a = (c.stiffness * c.spin_gain) * w;
    let s = (-c.damping * c.spin_gain) * w;
    let av = b.ang_vel;
    let l = b.ang_impulse;
    b.ang_impulse = s * av + a * c.spin + l;
}

fn apply_impulses(b: &mut RigidBody) {
    b.vel += b.impulse;
    let mut l = b.ang_momentum + b.ang_impulse;
    let len = l.length();
    let cap = b.min_inertia * 8.0;
    if len > cap {
        let s = cap / len;
        l *= s;
    }
    b.ang_momentum = l;
    b.ang_vel = angular_velocity(&b.rot, l, b.inv_inertia);
}
