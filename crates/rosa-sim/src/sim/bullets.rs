use glam::{IVec3, Vec3};
use rosa_math::vector::Vector;
use rosa_protocol::clientbound::game::{
    ItemKind,
    events::{Event, ServerEvent, bullet_hit::EventBulletHit, bullet_hole::EventBulletHole, update_vehicle::EventUpdateVehicle},
};

use super::{
    Sim,
    vehicles::{TYRE_BURST, WINDOW_BROKEN},
};
use crate::{
    PlayerId,
    human::{
        BoneId,
        damage::{BoneHit, damage_human, trace_ray_human},
        physics::{HumanOutput, add_bullet_hole},
    },
    vehicle::physics::{VehicleHit, VehiclePart, trace_vehicle_parts},
    world::{
        capsule::CapsuleHit,
        trace::{LevelHit, line_intersect_level},
    },
};

pub const MAX_BULLETS: usize = 16384;
const LIFETIME: i32 = 600;
const BULLET_GRAVITY: f32 = 0.0027222224;
const HIT_WORLD: i32 = 0;
const HIT_BODY: i32 = 1;
const HIT_VEHICLE: i32 = 2;
const VEHICLE_DAMAGE_SCALE: f32 = 1.5;
const TYRE_BURST_RADIUS: f32 = 0.75;
const DAMAGE_SCALE: f32 = 5.5;
const TICKS_PER_SECOND: f32 = crate::time::TICKS_PER_SECOND as f32;
const DAMAGE_FLOOR: f32 = 2.0;
const LIGHT_BONE: f32 = 6.0;
const LIGHT_BONE_MASS: f32 = 3.0;
const IMPULSE: f32 = 3.0;

/// bullet_data: each bullet type's mass (damage and recoil) and drag.
pub const BULLET_DATA: [(f32, f32); 4] = [(0.01, 0.004560367), (0.0062, 0.0039160457), (0.008, 0.007952156), (0.016, 0.005832072)];

/// A bullet in flight (bullets, 0x5c each).
#[derive(Clone, Debug)]
pub struct Bullet {
    pub kind: i32,
    /// Ticks left (+0x04); a hit sets it to 0.
    pub time: i32,
    pub player: Option<PlayerId>,
    pub mass: f32,
    /// Taken from the vertical speed every tick (+0x10): create_bullet stores the bytes "Pg2;", a little under
    /// 9.8 / 60².
    pub gravity: f32,
    /// Where it was before this tick's move (+0x20) and where it is now (+0x14).
    pub prev: Vec3,
    pub pos: Vec3,
    pub vel: Vec3,
}

/// create_bullet: a bullet of `kind` leaving `pos` at `vel`, fired by `player`.
pub fn create_bullet(bullets: &mut Vec<Bullet>, kind: i32, pos: Vec3, vel: Vec3, player: Option<PlayerId>, gravity_scale: f32) -> bool {
    if bullets.len() >= MAX_BULLETS {
        return false;
    }
    let mass = BULLET_DATA.get(kind as usize).map_or(0.0, |d| d.0);
    bullets.push(Bullet { kind, time: LIFETIME, player, mass, gravity: BULLET_GRAVITY * gravity_scale, prev: pos, pos, vel });
    true
}

/// What a bullet's path ran into this tick.
enum Target {
    Human(usize, BoneHit),
    Vehicle(usize, VehicleHit),
    Item(usize),
}

/// One tick of flight: gravity, then drag against the speed squared, then the move. Returns where it went from and to.
fn fly(b: &mut Bullet) -> (Vec3, Vec3) {
    b.prev = b.pos;
    b.vel.y -= b.gravity;
    let v = b.vel;
    let speed = ((v.y * v.y + v.x * v.x) + v.z * v.z).sqrt();
    let drag = BULLET_DATA.get(b.kind as usize).map_or(0.0, |d| d.1);
    let dir = if speed != 0.0 { v * (1.0 / speed) } else { Vec3::ZERO };
    b.vel = dir * ((-speed * speed) * drag) + v;
    b.pos = b.vel + b.pos;
    (b.prev, b.pos)
}

impl Sim {
    /// create_bullet for a bullet nobody fired.
    pub fn spawn_bullet(&mut self, kind: i32, pos: Vec3, vel: Vec3) {
        if create_bullet(&mut self.bullets, kind, pos, vel, None, self.bodies.gravity_scale) {
            self.stats.bullets += 1;
        }
    }

    /// A bullet stopped by a vehicle: a tyre loses health and bursts, a window breaks, or the vehicle is hurt.
    fn bullet_hit_vehicle(&mut self, i: usize, vid: usize, hit: VehicleHit) {
        let b = self.bullets[i].clone();
        self.bullets[i].time = 0;
        let v = b.vel;
        let speed = v.length();
        let damage = ((b.mass * (speed * VEHICLE_DAMAGE_SCALE)) * TICKS_PER_SECOND - DAMAGE_FLOOR) as i32;
        let damage = if damage >= 0 { damage } else { 0 };
        let Some(vehicle) = self.vehicles.get_mut(vid) else { return };
        let update = |kind: i32, part: usize| EventUpdateVehicle { vehicle_id: vid as i32, kind, part: part as i32, pos: Vector(hit.pos), velocity: Vector(v) };
        let event = match hit.part {
            VehiclePart::Wheel(k) => {
                let Some(w) = vehicle.wheels.get_mut(k) else { return };
                if w.health <= 0 {
                    return;
                }
                w.health -= damage;
                if w.health > 0 {
                    return;
                }
                w.drive |= 1;
                w.radius *= TYRE_BURST_RADIUS;
                w.popped = 1;
                ServerEvent::UpdateVehicle(update(TYRE_BURST, k))
            }
            _ if vehicle.kind == rosa_protocol::clientbound::game::VehicleKind::Train => return,
            VehiclePart::Window(k) => {
                if let Some(w) = vehicle.broken_windows.get_mut(k) {
                    *w = true;
                }
                ServerEvent::UpdateVehicle(update(WINDOW_BROKEN, k))
            }
            VehiclePart::Body(_) => {
                let e = EventBulletHit { unk: 0, hit_type: HIT_VEHICLE, pos: Vector(hit.pos), normal: Vector(hit.normal) };
                self.events.push(Event { tick_created: self.tick, kind: ServerEvent::BulletHit(e) });
                self.vehicle_take_damage(vid, damage);
                return;
            }
        };
        self.events.push(Event { tick_created: self.tick, kind: event });
    }

    /// bullet_simulation: every bullet slows down with drag and moves; the nearest human bone, vehicle, item or level
    /// surface on the way stops it.
    pub fn bullet_simulation(&mut self) {
        for i in 0..self.bullets.len() {
            let (from, to) = fly(&mut self.bullets[i]);
            let (best, target, found) = self.bullet_target(from, to, self.bullets[i].player);
            let map = &self.world.map;
            if let Some(level) = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, from, to)
                && best > level.hit.fraction
            {
                self.bullet_hit_level(i, level);
                continue;
            }
            match target {
                Some(Target::Vehicle(vid, hit)) => self.bullet_hit_vehicle(i, vid, hit),
                Some(Target::Item(id)) => self.bullet_hit_item(i, id, &found),
                Some(Target::Human(id, hit)) => self.bullet_hit_human(i, id, hit),
                None => {}
            }
        }
    }

    /// The nearest human bone, vehicle part or item hull between `from` and `to`, and its fraction of the way (1 for
    /// none). A shooter's own forearms and hands and god-mode players are passed through. Also returns the items
    /// found near the path.
    fn bullet_target(&self, from: Vec3, to: Vec3, shooter: Option<PlayerId>) -> (f32, Option<Target>, Vec<usize>) {
        let mut best = 1.0f32;
        let mut target = None;

        for (id, h) in self.humans.iter() {
            let Some(hit) = trace_ray_human(h, from, to, 0.0) else { continue };
            if !(best > hit.fraction) {
                continue;
            }
            if h.player == shooter && matches!(hit.bone, BoneId::ForearmLeft | BoneId::HandLeft | BoneId::ForearmRight | BoneId::HandRight) {
                continue;
            }
            if h.player.and_then(|p| self.players.get(p.idx())).is_some_and(|p| p.god_mode) {
                continue;
            }
            best = hit.fraction;
            target = Some(Target::Human(id, hit));
        }

        for (vid, v) in self.vehicles.iter() {
            let Some(t) = self.vehicle_types.get(v.kind as usize) else { continue };
            if let Some(hit) = trace_vehicle_parts(v, t, from, to, true)
                && !(best <= hit.fraction)
            {
                best = hit.fraction;
                target = Some(Target::Vehicle(vid, hit));
            }
        }

        let block = |a: f32, b: f32| {
            let (p, q) = ((0.25 * a) as i32, (0.25 * b) as i32);
            if a <= b { (p, q) } else { (q, p) }
        };
        let ((x0, x1), (y0, y1), (z0, z1)) = (block(from.x, to.x), block(from.y, to.y), block(from.z, to.z));
        let found = self.item_grid.query(IVec3::new(x0, y0, z0), IVec3::new(x1, y1, z1));
        for &id in &found {
            let Some(item) = self.items.get(id) else { continue };
            let Some(hull) = &self.item_types[item.item_type as usize].hull else { continue };
            let Some(world) = self.bodies.get(item.body).map(|b| hull.world_verts(b.pos, &b.rot)) else { continue };
            if let Some((fraction, _, _)) = hull.trace_mesh(&world, from, to)
                && !(best <= fraction)
            {
                best = fraction;
                target = Some(Target::Item(id));
            }
        }

        (best, target, found)
    }

    /// A bullet stopped by the level: an item-set cell makes its items real (a watermelon hit bursts); otherwise glass
    /// gets a hole or the surface shows a hit.
    fn bullet_hit_level(&mut self, i: usize, level: LevelHit) {
        let b = &mut self.bullets[i];
        b.pos = level.hit.pos;
        b.time = 0;
        let vel = b.vel;
        if let Some(set) = level.set {
            for (index, id) in self.spawn_hit_set(set, level.block) {
                if index == set.index
                    && let Some(item) = self.items.get_mut(id).filter(|i| i.item_type == ItemKind::Watermelon)
                {
                    item.health = 0;
                }
            }
            return;
        }
        let hole = CapsuleHit {
            pos: level.hit.pos,
            normal: level.hit.normal,
            dist: 0.0,
            area: level.area,
            block: level.block,
            cell: level.cell,
            face_attr: level.face_attr,
            set: level.set,
        };
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
    }

    /// A bullet stopped by an item: a watermelon bursts, and every item near the path wakes up.
    fn bullet_hit_item(&mut self, i: usize, hit_id: usize, found: &[usize]) {
        if let Some(item) = self.items.get_mut(hit_id).filter(|i| i.item_type == ItemKind::Watermelon) {
            item.health = 0;
        }
        for &id in found {
            if let Some(item) = self.items.get_mut(id) {
                item.physics_settled = false;
                item.settled_timer = 0;
            }
        }
        self.bullets[i].time = 0;
    }

    /// A bullet stopped by a human: damage from its mass and speed (more on the head) and a push on the bone hit.
    fn bullet_hit_human(&mut self, i: usize, id: usize, hit: BoneHit) {
        let b = self.bullets[i].clone();
        let v = b.vel;
        let mut damage = (b.mass * (v.length() * DAMAGE_SCALE) * TICKS_PER_SECOND - DAMAGE_FLOOR) as i32;
        if damage > 0 {
            let e = EventBulletHit { unk: 0, hit_type: HIT_BODY, pos: Vector(hit.pos), normal: Vector(hit.normal) };
            self.events.push(Event { tick_created: self.tick, kind: ServerEvent::BulletHit(e) });
            if hit.bone == BoneId::Head {
                damage = if damage <= 24 { if damage < 10 { damage * 2 } else { damage + damage * 2 } } else { damage << 2 };
            }
        } else {
            damage = 0;
        }
        let victim = self.humans.get(id).and_then(|h| h.player);
        self.score_hurt(b.player, victim, damage);
        let Some(h) = self.humans.get_mut(id) else { return };
        if h.is_immortal && h.down_timer > 0 {
            damage = 0;
        }
        damage_human(h, hit.bone, damage);
        let bone = &h.bones[hit.bone.index()];
        let weight = if LIGHT_BONE > bone.mass { LIGHT_BONE_MASS } else { 0.5 * bone.mass };
        let f = IMPULSE * (b.mass / (weight + b.mass));
        self.bullets[i].time = 0;
        let Some(body) = self.bodies.get_mut(bone.body) else { return };
        body.vel = f * v + body.vel;
        let r = hit.pos - body.pos;
        body.ang_momentum = v.cross(r) * f + body.ang_momentum;
    }

    /// bullet_TTL: every bullet loses a tick of life and is gone at 0, the last bullet taking its place (and counted
    /// down in the same pass).
    pub fn bullet_ttl(&mut self) {
        let mut i = 0;
        while i < self.bullets.len() {
            let b = &mut self.bullets[i];
            if b.time <= 65535 {
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
