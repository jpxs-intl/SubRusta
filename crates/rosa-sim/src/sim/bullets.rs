use glam::Vec3;
use rosa_math::vector::Vector;
use rosa_protocol::clientbound::game::events::{Event, ServerEvent, bullet_hit::EventBulletHit, bullet_hole::EventBulletHole};

use super::Sim;
use crate::{
    PlayerId,
    human::{
        damage::{damage_human, trace_ray_human},
        physics::{HumanOutput, add_bullet_hole},
    },
    world::{capsule::CapsuleHit, trace::line_intersect_level},
};

pub const MAX_BULLETS: usize = 0x4000;
const LIFETIME: i32 = 600;
const HIT_WORLD: i32 = 0;
const HIT_BODY: i32 = 1;
const HEAD: usize = 3;
const DAMAGE_SCALE: f32 = 5.5;
const TICKS_PER_SECOND: f32 = 60.0;
const DAMAGE_FLOOR: f32 = 2.0;
const LIGHT_BONE: f32 = 6.0;
const LIGHT_BONE_MASS: f32 = 3.0;
const IMPULSE: f32 = 3.0;

/// bullet_data: each bullet type's mass (damage and recoil) and drag.
pub const BULLET_DATA: [(f32, f32); 4] = [
    (f32::from_bits(0x3c23d70a), f32::from_bits(0x3b956f22)),
    (f32::from_bits(0x3bcb295f), f32::from_bits(0x3b80522c)),
    (f32::from_bits(0x3c03126f), f32::from_bits(0x3c0249c2)),
    (f32::from_bits(0x3c83126f), f32::from_bits(0x3bbf1af7)),
];

/// A bullet in flight (bullets, 0x5c each).
#[derive(Clone, Debug)]
pub struct Bullet {
    pub kind: i32,
    /// Ticks left (+0x04); a hit sets it to 0.
    pub time: i32,
    pub player: Option<PlayerId>,
    pub mass: f32,
    // TODO: name once its writers are ported (+0x10, subtracted from the vertical speed every tick, always 0 here)
    pub unk_10: f32,
    /// Where it was before this tick's move (+0x20) and where it is now (+0x14).
    pub prev: Vec3,
    pub pos: Vec3,
    pub vel: Vec3,
}

/// create_bullet: a bullet of `kind` leaving `pos` at `vel`, fired by `player`.
pub fn create_bullet(bullets: &mut Vec<Bullet>, kind: i32, pos: Vec3, vel: Vec3, player: Option<PlayerId>) {
    // TODO: create_bullet also keeps two axes across the flight direction (+0x38, +0x44) that nothing here reads
    if bullets.len() >= MAX_BULLETS {
        return;
    }
    let mass = BULLET_DATA.get(kind as usize).map_or(0.0, |d| d.0);
    bullets.push(Bullet { kind, time: LIFETIME, player, mass, unk_10: 0.0, prev: pos, pos, vel });
}

impl Sim {
    /// bullet_simulation: every bullet slows down with drag and moves; the nearest human bone or level surface on
    /// the way stops it. A human takes damage (more on the head) and a push; the level shows a hit.
    pub(crate) fn bullet_simulation(&mut self) {
        // TODO: vehicles and items (check_object_collisions, trace_segment_item_mesh: watermelons burst, items
        // wake) are tested between humans and the level; level hits on item-set cells spawn their items
        for i in 0..self.bullets.len() {
            let b = &mut self.bullets[i];
            let vy = b.vel.y - b.unk_10;
            let (vx, vz) = (b.vel.x, b.vel.z);
            b.prev = b.pos;
            b.vel.y = vy;
            let speed = ((vy * vy + vx * vx) + vz * vz).sqrt();
            let drag = BULLET_DATA.get(b.kind as usize).map_or(0.0, |d| d.1);
            let k = (-speed * speed) * drag;
            let dir = if speed != 0.0 {
                let inv = 1.0 / speed;
                Vec3::new(inv * vx, inv * vy, inv * vz)
            } else {
                Vec3::ZERO
            };
            b.vel = Vec3::new(vx + dir.x * k, vy + dir.y * k, vz + k * dir.z);
            b.pos = Vec3::new(b.vel.x + b.pos.x, b.vel.y + b.pos.y, b.vel.z + b.pos.z);
            let (from, to, shooter) = (b.prev, b.pos, b.player);

            let mut best = 1.0f32;
            let mut human_hit = None;
            for (id, h) in self.humans.iter() {
                let Some(hit) = trace_ray_human(h, from, to, 0.0) else { continue };
                if !(best > hit.fraction) {
                    continue;
                }
                if h.player == shooter && matches!(hit.bone, 5 | 6 | 8 | 9) {
                    continue;
                }
                // TODO: players in god mode are not hit
                best = hit.fraction;
                human_hit = Some((id, hit));
            }

            let map = &self.world.map;
            if let Some(level) = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, from, to)
                && best > level.hit.fraction
            {
                // TODO: a hit on an item-set cell spawns that cell's items (spawn_item_from_grid_cell), bursting a
                // watermelon it hit, instead of adding a bullet hole
                let vel = self.bullets[i].vel;
                let hole = CapsuleHit { pos: level.hit.pos, normal: level.hit.normal, dist: 0.0, area: level.area, block: level.block, cell: level.cell, face_attr: level.face_attr };
                let mut out = Vec::new();
                let kind = if add_bullet_hole(&mut self.world.map, hole, level.hit.pos, vel, &mut out)
                    && let Some(HumanOutput::Glass(b)) = out.pop()
                {
                    ServerEvent::BulletHole(EventBulletHole {
                        area: b.area,
                        block_x: b.block.x,
                        block_y: b.block.y,
                        block_z: b.block.z,
                        cell: b.cell,
                        face: b.face,
                        pos: Vector(b.pos),
                        vel: Vector(b.vel),
                    })
                } else {
                    ServerEvent::BulletHit(EventBulletHit { unk: 0, hit_type: HIT_WORLD, pos: Vector(level.hit.pos), normal: Vector(level.hit.normal) })
                };
                self.events.push(Event { tick_created: self.tick, kind });
                let b = &mut self.bullets[i];
                b.pos = level.hit.pos;
                b.time = 0;
                continue;
            }

            let Some((id, hit)) = human_hit else { continue };
            let b = self.bullets[i].clone();
            let v = b.vel;
            let speed = ((v.x * v.x + v.y * v.y) + v.z * v.z).sqrt();
            let mut damage = (b.mass * (speed * DAMAGE_SCALE) * TICKS_PER_SECOND - DAMAGE_FLOOR) as i32;
            if damage > 0 {
                let e = EventBulletHit { unk: 0, hit_type: HIT_BODY, pos: Vector(hit.pos), normal: Vector(hit.normal) };
                self.events.push(Event { tick_created: self.tick, kind: ServerEvent::BulletHit(e) });
                if hit.bone == HEAD {
                    damage = if damage <= 24 {
                        if damage < 10 { damage * 2 } else { damage + damage * 2 }
                    } else {
                        damage << 2
                    };
                }
            } else {
                damage = 0;
            }
            // TODO: handle_criminal_rating for shooting another player, punish_team_kill for a teammate (not in
            // eliminator)
            let Some(h) = self.humans.get_mut(id) else { continue };
            if h.unk_68 != 0 && h.unk_6c > 0 {
                damage = 0;
            }
            damage_human(h, hit.bone, damage);
            let bone = &h.bones[hit.bone];
            let mass = bone.mass;
            let weight = if LIGHT_BONE > mass { LIGHT_BONE_MASS } else { 0.5 * mass };
            let f = IMPULSE * (b.mass / (weight + b.mass));
            self.bullets[i].time = 0;
            let Some(body) = self.bodies.get_mut(bone.body) else { continue };
            body.vel = Vec3::new(f * v.x + body.vel.x, f * v.y + body.vel.y, f * v.z + body.vel.z);
            let r = Vec3::new(hit.pos.x - body.pos.x, hit.pos.y - body.pos.y, hit.pos.z - body.pos.z);
            let l = body.ang_momentum;
            body.ang_momentum = Vec3::new(
                (r.z * v.y - r.y * v.z) * f + l.x,
                (v.z * r.x - r.z * v.x) * f + l.y,
                f * (r.y * v.x - v.y * r.x) + l.z,
            );
        }
    }

    /// bullet_TTL: every bullet loses a tick of life and is gone at 0, the last bullet taking its place.
    pub(crate) fn bullet_ttl(&mut self) {
        let mut i = 0;
        while i < self.bullets.len() {
            let b = &mut self.bullets[i];
            if b.time <= 0xffff {
                b.time -= 1;
                if b.time <= 0 {
                    self.bullets.swap_remove(i);
                    continue;
                }
            }
            i += 1;
        }
    }
}
