use glam::{IVec3, Vec3};
use rosa_math::vector::Vector;
use rosa_physics::{
    RotMatrix, Table, body::RigidBodyType, rotation::{IDENTITY, rot_matrix_to_quaternion, rotate_orientation},
};
use rosa_protocol::clientbound::game::{ItemKind, ServerItemObject};

use super::{
    Sim,
    humans::{HUMAN_SLOTS, OBJECT_SLOTS},
    item_grid::ItemGrid,
    item_types::ItemType,
};
use crate::{PlayerId, rng::random_unit, world::trace::line_intersect_level};

pub const MAX_ITEMS: usize = 1024;

const SETTLE_SPEED: f32 = f32::from_bits(0x3b88_8889);
const SETTLE_TICKS: i32 = 60;
const CONTACT_FRICTION: f32 = f32::from_bits(0x3f19_999a);
const CONTACT_DEPTH_SCALE: f32 = 1.0 / 64.0;
const ITEM_FRICTION: f32 = f32::from_bits(0x3ecc_cccd);
const CONTACT_SOFTNESS: f32 = 1.0 / 32.0;
const SPAWN_IMPULSE: f32 = 0.05;
const SPAWN_SPIN_OFFSET: f32 = 0.1;
const SPAWN_DISTANCE: f32 = 1.5;

fn random_direction() -> Vec3 {
    loop {
        let v = Vec3::new(random_unit(), random_unit(), random_unit()) * 2.0 - 1.0;
        let len = v.length();
        if len > 0.001 && len <= 1.0 {
            return v / len;
        }
    }
}

pub struct Item {
    pub item_type: ItemKind,
    pub body: usize,
    pub pos2: Vec3,
    pub physics_sim: bool,
    pub physics_settled: bool,
    pub settled_timer: i32,
    pub despawn_time: i32,
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
    pub block_min: IVec3,
    pub block_max: IVec3,
    pub parent_human: i32,
    pub parent_item: i32,
    pub parent_slot: i32,
    /// Where a pocketed item was last snapped to (active_item_pos_rot_snap); its body takes this place next tick.
    pub pocket_pose: Option<(Vec3, RotMatrix)>,
}

/// What a human needs to collide with items: the broadphase grid, the items and their hulls.
pub struct Touchables<'a> {
    pub grid: &'a ItemGrid,
    pub items: &'a mut Table<Item>,
    pub types: &'a [ItemType],
}

impl Sim {
    pub fn create_item(&mut self, item_type: ItemKind, pos: Vec3, vel: Option<Vec3>, rot: RotMatrix) -> Option<usize> {
        let ty = &self.item_types[item_type as usize];
        let inertia = Vec3::new(1.0 / ty.inv_inertia.x, 1.0 / ty.inv_inertia.y, 1.0 / ty.inv_inertia.z);
        let body = self.bodies.create(RigidBodyType::Item, pos, rot, vel, inertia, ty.mass)?;

        let item = Item {
            item_type,
            body,
            pos2: pos,
            physics_sim: false,
            physics_settled: false,
            settled_timer: 0,
            despawn_time: 65535,
            aabb_min: Vec3::ZERO,
            aabb_max: Vec3::ZERO,
            block_min: IVec3::ZERO,
            block_max: IVec3::ZERO,
            parent_human: -1,
            parent_item: -1,
            parent_slot: 0,
            pocket_pose: None,
        };

        let Some(id) = self.items.insert(item) else {
            self.bodies.remove(body);
            return None;
        };

        if let Some(b) = self.bodies.get_mut(body) {
            b.owner = id as i32;
        }

        Some(id)
    }

    pub fn delete_item(&mut self, id: usize) {
        if let Some(item) = self.items.remove(id) {
            self.bodies.remove(item.body);
        }
    }

    pub(crate) fn spawn_watermelon_for(&mut self, player_id: PlayerId) {
        let Some(player) = self.players.get(player_id.idx()) else { return };

        let mut view = IDENTITY;
        rotate_orientation(&mut view, Vec3::Y, player.view_yaw);

        let right = -view[2];
        rotate_orientation(&mut view, right, player.view_pitch);

        let pos = player.camera_pos.0 - view[0] * SPAWN_DISTANCE;
        let Some(id) = self.create_item(ItemKind::Watermelon, pos, None, IDENTITY) else {
            return println!("[Sim] Item table full");
        };

        let body = self.items.get(id).unwrap().body;
        //self.bodies.add_impulse(body, random_direction() * SPAWN_SPIN_OFFSET, random_direction() * SPAWN_IMPULSE);

        println!("[Sim] Spawned watermelon #{id} at {pos:?}");
    }

    pub fn item_body(&self, id: usize) -> Option<(&Item, &rosa_physics::RigidBody)> {
        let item = self.items.get(id)?;
        Some((item, self.bodies.get(item.body)?))
    }

    pub fn physics_tick(&mut self) {
        self.bodies.simulate();
        self.sync_humans();
        self.sync_items_from_bodies();
        self.rebuild_item_grid();
        self.simulate_humans();
        self.item_simulation();
        self.logic_item();
        self.bodies.solve_bonds();
        self.cleanup_items();
    }

    fn sync_items_from_bodies(&mut self) {
        let map = &self.world.map;

        for (_, item) in self.items.iter_mut() {
            if !item.physics_sim {
                continue;
            }

            let Some(body) = self.bodies.get_mut(item.body) else { continue };
            let from = item.pos2;
            if let Some(h) = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, from, body.pos) {
                let t = 0.975 * h.hit.fraction;
                let d = body.pos - from;
                body.pos = Vec3::new(d.x * t + from.x, d.y * t + from.y, t * d.z + from.z);
            }
            item.pos2 = body.pos;

            let (pos, [r0, r1, r2]) = (body.pos, body.rot);
            let (mut mn, mut mx) = ([65536.0f32; 3], [-65536.0f32; 3]);
            for v in self.item_types[item.item_type as usize].hull.iter().flat_map(|h| &h.verts) {
                let p = [
                    ((pos.x + v.x * r0.x) + v.y * r1.x) + r2.x * v.z,
                    ((r0.y * v.x + pos.y) + r1.y * v.y) + r2.y * v.z,
                    ((r0.z * v.x + pos.z) + r1.z * v.y) + v.z * r2.z,
                ];
                for k in 0..3 {
                    if mn[k] > p[k] {
                        mn[k] = p[k];
                    }
                    if p[k] > mx[k] {
                        mx[k] = p[k];
                    }
                }
            }
            item.aabb_min = Vec3::from_array(mn);
            item.aabb_max = Vec3::from_array(mx);
            item.block_min = IVec3::from_array(mn.map(|v| (v * 0.25) as i32));
            item.block_max = IVec3::from_array(mx.map(|v| (v * 0.25) as i32));
        }
    }

    fn rebuild_item_grid(&mut self) {
        self.item_grid.clear();
        for (id, item) in self.items.iter() {
            if self.item_types[item.item_type as usize].can_collide {
                self.item_grid.insert(id, item.block_min, item.block_max);
            }
        }
    }

    fn item_simulation(&mut self) {
        for id in self.items.ids() {
            let Some(item) = self.items.get_mut(id) else { continue };
            if !(-32.0 <= item.pos2.y) {
                item.despawn_time = 0;
            }
            item.physics_sim = false;
            let (body_id, item_type, parent) = (item.body, item.item_type, item.parent_human);
            let holder = (parent != -1).then(|| self.humans.get(parent as usize)).flatten();
            // TODO: items hanging off another item and the isInPocket flag
            if let Some(h) = holder.filter(|_| item.parent_slot > 1) {
                let vel = h.bones[0].vel;
                item.physics_settled = false;
                item.settled_timer = 0;
                let pose = item.pocket_pose;
                if let Some(body) = self.bodies.get_mut(body_id) {
                    if let Some((pos, rot)) = pose {
                        body.pos = pos;
                        body.rot = rot;
                        item.pos2 = pos;
                    }
                    body.vel = vel;
                    body.ang_momentum = Vec3::ZERO;
                    body.ang_vel = Vec3::ZERO;
                    body.settled = true;
                }
                continue;
            }
            if let Some(h) = holder {
                if item.physics_settled {
                    if let Some(body) = self.bodies.get_mut(body_id) {
                        body.vel = Vec3::ZERO;
                        body.ang_vel = Vec3::ZERO;
                        body.ang_momentum = Vec3::ZERO;
                        body.settled = true;
                    }
                    continue;
                }
                item.physics_sim = true;
                item.settled_timer = 0;
                let seated = h.vehicle.is_some();
                if let Some(body) = self.bodies.get_mut(body_id) {
                    body.settled = false;
                }
                if self.item_types[item_type as usize].can_collide {
                    self.item_update_collision_state(id);
                }
                if !seated {
                    self.bounding_box_contacts(body_id, item_type);
                }
                continue;
            }
            if item.physics_settled {
                if let Some(body) = self.bodies.get_mut(body_id) {
                    body.vel = Vec3::ZERO;
                    body.ang_vel = Vec3::ZERO;
                    body.ang_momentum = Vec3::ZERO;
                    body.settled = true;
                }
            } else {
                item.physics_sim = true;
                if let Some(body) = self.bodies.get_mut(body_id) {
                    body.settled = false;
                }
                if self.item_types[item_type as usize].can_collide {
                    self.item_update_collision_state(id);
                }
                self.bounding_box_contacts(body_id, item_type);
                let Some(body) = self.bodies.get(body_id) else { continue };
                let (v, w) = (body.vel, body.ang_vel);
                let item = self.items.get_mut(id).unwrap();
                let speed = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
                let spin = (w.y * w.y + w.x * w.x + w.z * w.z).sqrt();
                if SETTLE_SPEED <= speed || SETTLE_SPEED <= spin {
                    item.settled_timer = 0;
                } else {
                    let t = item.settled_timer;
                    if t < SETTLE_TICKS {
                        item.settled_timer = t + 1;
                    }
                    if t >= SETTLE_TICKS - 1 {
                        item.physics_settled = true;
                    }
                }
            }
        }
    }

    /// logic_item: snaps every pocketed item to its human's chest (active_item_pos_rot_snap).
    // TODO: the rest of logic_item (phones, grenades, computers, cash and the other item behaviours)
    fn logic_item(&mut self) {
        for id in self.items.ids() {
            let item = self.items.get_mut(id).unwrap();
            if item.parent_human == -1 {
                continue;
            }
            if item.parent_slot as u32 <= 1 {
                item.pocket_pose = None;
                continue;
            }
            let Some(h) = self.humans.get(item.parent_human as usize) else { continue };
            let (chest, pelvis) = (&h.bones[2], &h.bones[0]);
            let [c0, c1, c2] = chest.rot;
            let rot = [c0, Vec3::new(-c2.x, -c2.y, -c2.z), c1];
            let [r0, r1, _] = rot;
            let p = chest.pos;
            let pos = Vec3::new(
                (r1.x * -0.25 + p.x) + r0.x * 0.125,
                (r1.y * -0.25 + p.y) + r0.y * 0.125,
                (-0.25 * r1.z + p.z) + 0.125 * r0.z,
            );
            item.pocket_pose = Some((pos, rot));
            // TODO: a seated human's pocket keeps the vehicle's position as the previous position
            item.pos2 = pelvis.pos;
            let vel = pelvis.vel;
            if let Some(body) = self.bodies.get_mut(item.body) {
                body.vel = vel;
                body.ang_momentum = Vec3::ZERO;
                body.ang_vel = Vec3::ZERO;
            }
        }
    }

    /// item_update_collision_state: hull contacts with every collidable item whose bounding box overlaps this one's.
    /// Each pair is handled once, by the lower item id, unless the other item has settled; touching wakes both items
    /// unless both have been still for a full second.
    fn item_update_collision_state(&mut self, id: usize) {
        let Some(item) = self.items.get(id) else { return };
        let (kind, body_a, min, max) = (item.item_type, item.body, item.aabb_min, item.aabb_max);
        for other in self.item_grid.query(item.block_min, item.block_max) {
            if other == id {
                continue;
            }
            let Some(o) = self.items.get(other) else { continue };
            if id > other && !o.physics_settled {
                continue;
            }
            if o.aabb_min.x > max.x || min.x > o.aabb_max.x || o.aabb_min.z > max.z || min.z > o.aabb_max.z || o.aabb_min.y > max.y || min.y > o.aabb_max.y {
                continue;
            }
            let (Some(ha), Some(hb)) = (&self.item_types[kind as usize].hull, &self.item_types[o.item_type as usize].hull) else { continue };
            let body_b = o.body;
            let (Some(a), Some(b)) = (self.bodies.get(body_a), self.bodies.get(body_b)) else { continue };
            let contacts = super::hull::collide_convex_hulls(ha, a.pos, &a.rot, hb, b.pos, &b.rot);
            for c in &contacts {
                let (first, second) = if c.swapped { (body_b, body_a) } else { (body_a, body_b) };
                self.bodies.add_body_contact(first, second, c.offset_a, c.offset_b, c.normal, c.depth, ITEM_FRICTION, CONTACT_DEPTH_SCALE, CONTACT_SOFTNESS);
            }
            if contacts.is_empty() {
                continue;
            }
            let o_timer = self.items.get(other).unwrap().settled_timer;
            if o_timer <= 59 || self.items.get(id).unwrap().settled_timer <= 59 {
                self.items.get_mut(id).unwrap().physics_settled = false;
                self.items.get_mut(other).unwrap().physics_settled = false;
            }
        }
    }

    fn bounding_box_contacts(&mut self, body_id: usize, item_type: ItemKind) {
        let bounds = self.item_types[item_type as usize].bounds;
        let Some(body) = self.bodies.get(body_id) else { return };
        let (pos, [r0, r1, r2]) = (body.pos, body.rot);
        let map = &self.world.map;
        for i in 0..8 {
            let q = i & 3;
            let sx = if q == 0 || q == 3 { -bounds.x } else { bounds.x };
            let sy = if i <= 3 { -bounds.y } else { bounds.y };
            let sz = if q <= 1 { -bounds.z } else { bounds.z };
            let corner = Vec3::new(
                ((pos.x + r0.x * sx) + r1.x * sy) + r2.x * sz,
                ((pos.y + r0.y * sx) + r1.y * sy) + r2.y * sz,
                ((sx * r0.z + pos.z) + sy * r1.z) + sz * r2.z,
            );
            let Some(h) = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, pos, corner) else { continue };
            let (hp, n) = (h.hit.pos, h.hit.normal);
            let offset = Vec3::new(hp.x - pos.x, hp.y - pos.y, hp.z - pos.z);
            let depth = (n.x * (hp.x - corner.x) + (hp.y - corner.y) * n.y) + (hp.z - corner.z) * n.z;
            self.bodies.add_world_contact(body_id, offset, n, depth, CONTACT_FRICTION, CONTACT_DEPTH_SCALE, CONTACT_SOFTNESS);
            // TODO: spawn_item_from_grid_cell when the ray hits an item-set cell
        }
    }

    fn cleanup_items(&mut self) {
        let mut dead = Vec::new();
        for (id, item) in self.items.iter_mut() {
            if item.despawn_time <= 65534 {
                item.despawn_time -= 1;
                if item.despawn_time <= 0 {
                    dead.push(id);
                }
            }
        }
        for id in dead {
            self.delete_item(id);
        }
    }

    pub(crate) fn item_objects(&self) -> Vec<ServerItemObject> {
        self.items
            .iter()
            .filter(|&(id, _)| HUMAN_SLOTS + id < OBJECT_SLOTS)
            .filter_map(|(id, item)| {
                let body = self.bodies.get(item.body)?;
                let rot = rot_matrix_to_quaternion(&body.rot);
                Some(ServerItemObject { slot: (HUMAN_SLOTS + id) as u16, item_id: id as u16, item_type: item.item_type, pos: Vector(body.pos), rot, parent_item: item.parent_item, parent_human: item.parent_human, parent_slot: item.parent_slot })
            })
            .collect()
    }
}
