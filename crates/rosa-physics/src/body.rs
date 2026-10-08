use glam::Vec3;

use crate::{
    Bond, Table,
    bond::MAX_BONDS,
    rotation::{RotMatrix, angular_velocity, rk4_rotation},
};

#[derive(Clone, Debug)]
#[repr(i32)]
pub enum RigidBodyType {
    HumanBone = 0,
    Vehicle = 1,
    Wheel = 2,
    Item = 3
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

pub struct RigidBodies {
    bodies: Table<RigidBody>,
    bonds: Table<Bond>,
}

impl Default for RigidBodies {
    fn default() -> Self {
        Self { bodies: Table::new(8192), bonds: Table::new(MAX_BONDS) }
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
        for (_, b) in self.bodies.iter_mut() {
            if b.settled {
                continue;
            }
            let vy = b.vel.y - (9.8 / (60.0 * 60.0));
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
        b.vel = Vec3::new(b.vel.x + j.x, b.vel.y + j.y, b.vel.z + j.z);
        b.ang_momentum = Vec3::new(
            (j.y * r.z - j.z * r.y) + b.ang_momentum.x,
            (j.z * r.x - r.z * j.x) + b.ang_momentum.y,
            (r.y * j.x - j.y * r.x) + b.ang_momentum.z,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_world_contact(&mut self, body: usize, offset: Vec3, normal: Vec3, depth: f32, friction: f32, depth_scale: f32, softness: f32) {
        self.bonds.insert(Bond::WorldContact(Contact { body, offset, normal, bias: depth * depth_scale, softness, friction }));
    }

    /// add_collision_rigidbody_on_rigidbody: a one-tick contact pushing `body_a` along `normal` and `body_b` against it.
    #[allow(clippy::too_many_arguments)]
    pub fn add_body_contact(&mut self, body_a: usize, body_b: usize, offset_a: Vec3, offset_b: Vec3, normal: Vec3, depth: f32, friction: f32, depth_scale: f32, softness: f32) {
        let (Some(a), Some(b)) = (self.bodies.get(body_a), self.bodies.get(body_b)) else { return };
        let inv = 1.0 / (a.mass + b.mass);
        let (weight_a, weight_b) = (b.mass * inv, a.mass * inv);
        self.bonds.insert(Bond::BodyContact(BodyContact { body_a, body_b, offset_a, offset_b, normal, bias: depth * depth_scale, softness, friction, weight_a, weight_b }));
    }

    pub fn solve_bonds(&mut self) {
        let Self { bodies, bonds } = self;
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
    let bias = c.bias;
    let ux = ((r.y * w.z - r.z * w.y) + v.x) * k + bias * n.x;
    let uy = ((r.z * w.x - w.z * r.x) + v.y) * k + bias * n.y;
    let uz = ((w.y * r.x - r.y * w.x) + v.z) * k + bias * n.z;
    let d = (ux * n.x + uy * n.y) + uz * n.z;
    if !(d > 0.0) {
        return;
    }
    let (mut jx, mut jy, mut jz) = (n.x * d, n.y * d, d * n.z);
    let (tx, ty, tz) = (ux - jx, uy - jy, uz - jz);
    let tlen = ((tx * tx + ty * ty) + tz * tz).sqrt();
    let mut slide = 0.0f32;
    if tlen > 0.0 {
        let inv = 1.0 / tlen;
        let (dx, dy, dz) = (tx * inv, ty * inv, tz * inv);
        let jlen = ((jx * jx + jy * jy) + jz * jz).sqrt();
        let limit = c.friction * jlen;
        let k = 2.5 * (9.8 / (60.0 * 60.0));
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
        jx += dx * m;
        jy += dy * m;
        jz += m * dz;
    }
    b.slide += slide;
    b.impulse = Vec3::new(b.impulse.x + jx, b.impulse.y + jy, b.impulse.z + jz);
    b.ang_impulse = Vec3::new(
        (r.z * jy - r.y * jz) + b.ang_impulse.x,
        (jz * r.x - r.z * jx) + b.ang_impulse.y,
        (r.y * jx - r.x * jy) + b.ang_impulse.z,
    );
}

fn solve_body_contact(bodies: &mut Table<RigidBody>, c: &BodyContact) {
    let (Some(a), Some(b)) = (bodies.get(c.body_a), bodies.get(c.body_b)) else { return };
    let (va, wa, vb, wb) = (a.vel, a.ang_vel, b.vel, b.ang_vel);
    let (ra, rb, n, s, bias) = (c.offset_a, c.offset_b, c.normal, c.softness, c.bias);
    let ns = -s;
    let ux = ((va.x + (ra.y * wa.z - wa.y * ra.z)) * ns + bias * n.x) + ((rb.y * wb.z - rb.z * wb.y) + vb.x) * s;
    let uy = ((va.y + (wa.x * ra.z - wa.z * ra.x)) * ns + bias * n.y) + ((rb.z * wb.x - wb.z * rb.x) + vb.y) * s;
    let uz = (((wa.y * ra.x - wa.x * ra.y) + va.z) * ns + bias * n.z) + ((wb.y * rb.x - wb.x * rb.y) + vb.z) * s;
    let d = (ux * n.x + uy * n.y) + uz * n.z;
    if !(d > 0.0) {
        return;
    }
    let (mut jx, mut jy, mut jz) = (n.x * d, n.y * d, n.z * d);
    let (tx, ty, tz) = (ux - jx, uy - jy, uz - jz);
    let tlen = ((tx * tx + ty * ty) + tz * tz).sqrt();
    if tlen > 0.0 {
        let inv = 1.0 / tlen;
        let (dx, dy, dz) = (tx * inv, ty * inv, tz * inv);
        let jlen = ((jx * jx + jy * jy) + jz * jz).sqrt();
        let limit = c.friction * jlen;
        let m = if limit < tlen { limit } else { tlen };
        jx += dx * m;
        jy += dy * m;
        jz += m * dz;
    }

    let (wa_, nwb) = (c.weight_a, -c.weight_b);
    if let Some(a) = bodies.get_mut(c.body_a) {
        a.impulse = Vec3::new(wa_ * jx + a.impulse.x, wa_ * jy + a.impulse.y, wa_ * jz + a.impulse.z);
        let l = a.ang_impulse;
        a.ang_impulse = Vec3::new(
            (ra.z * jy - ra.y * jz) * wa_ + l.x,
            (ra.x * jz - ra.z * jx) * wa_ + l.y,
            (ra.y * jx - ra.x * jy) * wa_ + l.z,
        );
    }
    if let Some(b) = bodies.get_mut(c.body_b) {
        b.impulse = Vec3::new(nwb * jx + b.impulse.x, nwb * jy + b.impulse.y, nwb * jz + b.impulse.z);
        let l = b.ang_impulse;
        b.ang_impulse = Vec3::new(
            (rb.z * jy - rb.y * jz) * nwb + l.x,
            (jz * rb.x - rb.z * jx) * nwb + l.y,
            (rb.y * jx - jy * rb.x) * nwb + l.z,
        );
    }
}

fn solve_ground_contact(b: &mut RigidBody, c: &GroundContact) {
    let (n, v, g) = (c.normal, b.vel, c.ground_vel);
    let push = c.depth * c.stiffness;
    let k = c.stiffness * 0.125;
    let nd = -c.damping;
    let ux = ((v.x + 0.0) - g.x) * nd + (c.drift.x * k + push * n.x);
    let uy = ((v.y + 0.0) - g.y) * nd + (c.drift.y * k + push * n.y);
    let uz = (k * c.drift.z + push * n.z) + ((v.z + 0.0) - g.z) * nd;
    let d = (ux * n.x + uy * n.y) + uz * n.z;
    if !(d > 0.0) {
        return;
    }
    let (mut jx, mut jy, mut jz) = (n.x * d, n.y * d, n.z * d);
    let (tx, ty, tz) = (ux - jx, uy - jy, uz - jz);
    let tl = ((tx * tx + ty * ty) + tz * tz).sqrt();
    if tl > 0.0 {
        let inv = 1.0 / tl;
        let (dx, dy, dz) = (tx * inv, ty * inv, tz * inv);
        let jl = ((jx * jx + jy * jy) + jz * jz).sqrt();
        let limit = c.friction * jl;
        let m = if limit < tl { limit } else { tl };
        jx += dx * m;
        jy += dy * m;
        jz += dz * m;
    }
    let w = d * 1024.0;
    let w = if 1.0 < w { 1.0 } else { w };
    b.impulse = Vec3::new(b.impulse.x + jx, c.lift * jy + b.impulse.y, jz + b.impulse.z);
    let a = (c.stiffness * c.spin_gain) * w;
    let s = (-c.damping * c.spin_gain) * w;
    let av = b.ang_vel;
    let l = b.ang_impulse;
    b.ang_impulse = Vec3::new(
        (s * av.x + a * c.spin.x) + l.x,
        (av.y * s + c.spin.y * a) + l.y,
        (av.z * s + c.spin.z * a) + l.z,
    );
}

fn apply_impulses(b: &mut RigidBody) {
    b.vel = Vec3::new(b.vel.x + b.impulse.x, b.vel.y + b.impulse.y, b.vel.z + b.impulse.z);
    let mut l = Vec3::new(b.ang_momentum.x + b.ang_impulse.x, b.ang_momentum.y + b.ang_impulse.y, b.ang_momentum.z + b.ang_impulse.z);
    let len = ((l.x * l.x + l.y * l.y) + l.z * l.z).sqrt();
    let cap = b.min_inertia * 8.0;
    if len > cap {
        let s = cap / len;
        l = Vec3::new(l.x * s, l.y * s, l.z * s);
    }
    b.ang_momentum = l;
    b.ang_vel = angular_velocity(&b.rot, l, b.inv_inertia);
}
