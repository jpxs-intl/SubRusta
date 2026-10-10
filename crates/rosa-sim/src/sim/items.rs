use glam::{IVec3, Vec3};
use rand::RngExt;
use rosa_math::vector::Vector;
use rosa_physics::{
    RotMatrix, Table, body::RigidBodyType, rotation::{IDENTITY, rot_matrix_to_quaternion, rotate_orientation},
};
use rosa_protocol::clientbound::game::{
    ItemKind, ItemTail, ServerItemObject,
    events::{Event, ServerEvent, bullet_hit::EventBulletHit},
};

use super::item_state::ItemState;
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
const ITEM_HEALTH: i32 = 100;
const ITEM_FRICTION: f32 = f32::from_bits(0x3ecc_cccd);
const CONTACT_SOFTNESS: f32 = 1.0 / 32.0;
/// The soccer ball: a sphere of 0.11 kicked by any bone but the arms' within 0.36 of it, and slowed spinning while it
/// touches the level.
const BALL_RADIUS: f32 = 0.11;
const KICK_REACH: f32 = 0.36;
const KICK_SKIPPED_BONES: [usize; 4] = [5, 6, 8, 9];
const KICK_FRICTION: f32 = f32::from_bits(0x3ecc_cccd);
const KICK_DEPTH_SCALE: f32 = 1.0 / 32.0;
const KICK_SOFTNESS: f32 = 1.0 / 16.0;
const BALL_SPIN_DAMPING: f32 = 0.9375;
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
    /// Hit points (+0x138, 100 at creation); an item at none is destroyed, a watermelon shot bursting.
    pub health: i32,
    pub pos2: Vec3,
    /// The body's velocity at the last sync (+0x80), what a burst sends.
    pub vel: Vec3,
    pub physics_sim: bool,
    pub physics_settled: bool,
    pub settled_timer: i32,
    /// Whether the item is out of sight (item +0x30): in a closed briefcase, on an item in a pocket, or in a pocket
    /// itself unless it is a phone or walkie-talkie. Clients are not sent pocketed items.
    pub in_pocket: bool,
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
    /// Items mounted on this one (+0x34 count, +0x38 ids), e.g. a gun's magazine; up to eight.
    pub children: Vec<usize>,
    /// The id in the first child slot (+0x38), which stays when the last child comes off (0 for a new item).
    pub first_child: usize,
    /// Where a mounted item was last placed on its parent by logic_item; its body takes this place next tick.
    pub mount_pose: Option<(Vec3, RotMatrix)>,
    /// What only items of this type have.
    pub state: ItemState,
    /// Keys pressed on the item this tick by the hand holding it (+0x150: 1 use, 2 secondary), and last tick (+0x154).
    pub input_flags: u32,
    pub last_input_flags: u32,
}

/// What a human needs to collide with items and vehicles: the broadphase grid, the items and their hulls, the
/// vehicles and their types.
pub struct Touchables<'a> {
    pub grid: &'a ItemGrid,
    pub items: &'a mut Table<Item>,
    pub types: &'a [ItemType],
    pub vehicles: &'a mut Table<crate::vehicle::Vehicle>,
    pub vehicle_types: &'a [crate::vehicle::types::VehicleType],
    /// The (vehicle, seat) pairs humans sit in, kept up to date as humans get in and out.
    pub occupied: Vec<(usize, usize)>,
}

impl Sim {
    pub fn create_item(&mut self, item_type: ItemKind, pos: Vec3, vel: Option<Vec3>, rot: RotMatrix) -> Option<usize> {
        let ty = &self.item_types[item_type as usize];
        let inertia = Vec3::new(1.0 / ty.inv_inertia.x, 1.0 / ty.inv_inertia.y, 1.0 / ty.inv_inertia.z);
        let body = self.bodies.create(RigidBodyType::Item, pos, rot, vel, inertia, ty.mass)?;

        let item = Item {
            item_type,
            body,
            health: ITEM_HEALTH,
            pos2: pos,
            vel: vel.unwrap_or(Vec3::ZERO),
            physics_sim: false,
            physics_settled: false,
            settled_timer: 0,
            in_pocket: false,
            despawn_time: 65535,
            aabb_min: Vec3::ZERO,
            aabb_max: Vec3::ZERO,
            block_min: IVec3::ZERO,
            block_max: IVec3::ZERO,
            parent_human: -1,
            parent_item: -1,
            parent_slot: 0,
            pocket_pose: None,
            children: Vec::new(),
            first_child: 0,
            mount_pose: None,
            state: ItemState::new(item_type, ty),
            input_flags: 0,
            last_input_flags: 0,
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

    /// delete_item: takes the item out of its holder's inventory first.
    pub fn delete_item(&mut self, id: usize) {
        // TODO: item-set grid bits, the counter at 0x45385f08 for type 0x1d, child items and a key's vehicle
        let parent = self.items.get(id).map_or(-1, |i| i.parent_human);
        let slot = self.items.get(id).map_or(0, |i| i.parent_slot as usize);
        if parent != -1
            && let Some(h) = self.humans.get_mut(parent as usize)
        {
            let mut touch = Touchables { grid: &self.item_grid, items: &mut self.items, types: &self.item_types, vehicles: &mut self.vehicles, vehicle_types: &self.vehicle_types, occupied: Vec::new() };
            crate::human::inventory::detach_item(h, &mut self.bodies, &mut touch, id, slot);
        }
        if let Some(item) = self.items.remove(id) {
            self.bodies.remove(item.body);
        }
    }

    pub(crate) fn spawn_watermelon_for(&mut self, player_id: PlayerId) {
        let mut rng = rand::rng();

        for _ in 0..100 {
            let rand_x = rng.random_range(1.0..=5.0);
            let rand_z = rng.random_range(1.0..=5.0);
            let rand_y = rng.random_range(1.0..=5.0);

            let mut v = Vec3::Z;
            v.x += rand_x;
            v.y += rand_y;
            v.z += rand_z;

            self.spawn_item_for(player_id, ItemKind::Watermelon, v);
        }
    }

    /// Test command: an item of any type in front of the player's camera.
    pub(crate) fn spawn_item_for(&mut self, player_id: PlayerId, kind: ItemKind, pos_offset: Vec3) -> Option<usize> {
        let player = self.players.get(player_id.idx())?;

        let mut view = IDENTITY;
        rotate_orientation(&mut view, Vec3::Y, player.view_yaw);

        let right = -view[2];
        rotate_orientation(&mut view, right, player.view_pitch);

        let mut pos = player.camera_pos.0 - view[0] * SPAWN_DISTANCE;
        pos += pos_offset;

        let Some(id) = self.create_item(kind, pos, None, IDENTITY) else {
            println!("[Sim] Item table full");
            return None;
        };
        println!("[Sim] Spawned {kind:?} #{id} at {pos:?}");
        Some(id)
    }

    /// Test command: every gun in a row in front of the player, each with three of its magazines beside it.
    pub(crate) fn spawn_guns_for(&mut self, player_id: PlayerId) {
        const GUNS: [(ItemKind, Option<ItemKind>); 7] = [
            (ItemKind::Ak47, Some(ItemKind::Ak47Mag)),
            (ItemKind::M16, Some(ItemKind::M16Mag)),
            (ItemKind::Mp5, Some(ItemKind::Mp5Mag)),
            (ItemKind::Uzi, Some(ItemKind::UziMag)),
            (ItemKind::Pistol, Some(ItemKind::PistolMag)),
            (ItemKind::Magnum, Some(ItemKind::MagnumMag)),
            (ItemKind::Auto5, None),
        ];
        const GUN_SPACING: f32 = 0.75;
        const MAG_SPACING: f32 = 0.25;
        let Some(player) = self.players.get(player_id.idx()) else { return };
        let mut view = IDENTITY;
        rotate_orientation(&mut view, Vec3::Y, player.view_yaw);
        let (forward, right) = (-view[0], -view[2]);
        let centre = player.camera_pos.0 + forward * SPAWN_DISTANCE;
        let first = -(GUNS.len() as f32 - 1.0) * 0.5;
        for (k, (gun, mag)) in GUNS.into_iter().enumerate() {
            let at = centre + right * ((first + k as f32) * GUN_SPACING);
            self.create_item(gun, at, None, view);
            for m in mag.into_iter().flat_map(|m| [m; 3]).enumerate() {
                self.create_item(m.1, at + forward * (MAG_SPACING * (m.0 as f32 + 1.0)), None, view);
            }
        }
    }

    pub fn item_mut(&mut self, id: usize) -> Option<&mut Item> {
        self.items.get_mut(id)
    }

    pub fn set_game_state(&mut self, state: rosa_protocol::clientbound::game::GameState) {
        self.gamestate = state;
    }

    pub fn item_body(&self, id: usize) -> Option<(&Item, &rosa_physics::RigidBody)> {
        let item = self.items.get(id)?;
        Some((item, self.bodies.get(item.body)?))
    }

    pub fn physics_tick(&mut self) {
        self.physics_step();
        self.physics_solve();
    }

    /// The first part of physics_simulation: the bodies moved, then vehicles, humans and items simulated.
    pub fn physics_step(&mut self) {
        for (_, h) in self.humans.iter_mut() {
            h.progress_bar = 0;
        }
        self.bodies.simulate();
        self.update_vehicle_bounds();
        self.sync_humans();
        self.sync_items_from_bodies();
        self.rebuild_item_grid();
        self.simulate_vehicles();
        self.simulate_humans();
        self.item_simulation();
        self.logic_item();
    }

    /// The rest of physics_simulation: the bonds solved and the items left over cleaned up.
    pub fn physics_solve(&mut self) {
        self.solve_bonds();
        self.cleanup_items();
        self.cleanup_vehicles();
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
            item.vel = body.vel;

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
            let in_pocket = self.items.get(id).is_some_and(|item| {
                let parent = usize::try_from(item.parent_item).ok().and_then(|p| self.items.get(p));
                parent.is_some_and(|p| p.item_type == ItemKind::Briefcase || (p.parent_human != -1 && p.parent_slot > 2))
                    || (item.parent_human != -1 && !matches!(item.item_type, ItemKind::Phone | ItemKind::Radio) && item.parent_slot > 2)
            });
            let Some(item) = self.items.get_mut(id) else { continue };
            item.in_pocket = in_pocket;
            if !(-32.0 <= item.pos2.y) {
                item.despawn_time = 0;
            }
            if item.health <= 0 {
                let e = EventBulletHit { unk: 0, hit_type: 0, pos: Vector(item.pos2), normal: Vector(item.vel) };
                self.events.push(Event { tick_created: self.tick, kind: ServerEvent::BulletHit(e) });
                item.despawn_time = 0;
            }
            item.physics_sim = false;
            let (body_id, item_type, parent) = (item.body, item.item_type, item.parent_human);
            let holder = (parent != -1).then(|| self.humans.get(parent as usize)).flatten();
            if item.parent_item != -1 {
                item.physics_settled = false;
                item.settled_timer = 0;
                item.vel = Vec3::ZERO;
                let pose = item.mount_pose;
                if let Some(body) = self.bodies.get_mut(body_id) {
                    if let Some((pos, rot)) = pose {
                        body.pos = pos;
                        body.rot = rot;
                        item.pos2 = pos;
                    }
                    body.vel = Vec3::ZERO;
                    body.ang_momentum = Vec3::ZERO;
                    body.ang_vel = Vec3::ZERO;
                    body.settled = true;
                }
                continue;
            }
            if let Some(h) = holder.filter(|_| item.parent_slot > 1) {
                let vel = h.bones[0].vel;
                item.vel = vel;
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
                if item_type == ItemKind::SoccerBall {
                    self.soccer_ball(id, body_id);
                } else if !seated {
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
                if item_type == ItemKind::SoccerBall {
                    self.soccer_ball(id, body_id);
                } else {
                    self.bounding_box_contacts(body_id, item_type);
                }
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

    /// logic_item: snaps every pocketed item to its human's chest (active_item_pos_rot_snap), then runs each item's
    /// behaviour.
    fn logic_item(&mut self) {
        self.snap_pocketed_items();
        self.place_mounted_items();
        self.item_behaviours();
    }

    /// The child part of logic_item: a mounted item sits on its parent, cash fanned out across an open briefcase and
    /// anything else 0.0625 below the parent's middle.
    fn place_mounted_items(&mut self) {
        const CASH_SPREAD: f32 = 0.1875;
        const CASH_SCALE: f32 = 0.75;
        const CASH_START: f32 = 0.2109375;
        const BELOW: f32 = -0.0625;
        for id in self.items.ids() {
            let item = self.items.get(id).unwrap();
            if item.parent_item == -1 {
                continue;
            }
            let (kind, slot) = (item.item_type, item.parent_slot);
            let Some(parent) = self.items.get(item.parent_item as usize) else { continue };
            let Some(body) = self.bodies.get(parent.body) else { continue };
            let (p, rot) = (body.pos, body.rot);
            let [r0, r1, _] = rot;
            let pos = match parent.item_type {
                ItemKind::Briefcase | ItemKind::BriefcaseOpen => {
                    if matches!(kind, ItemKind::CashRound | ItemKind::CashWorld) {
                        let k = slot as f32 * CASH_SPREAD * CASH_SCALE * CASH_SCALE - CASH_START;
                        Vec3::new(p.x + r0.x * k, r0.y * k + p.y, k * r0.z + p.z)
                    } else {
                        p
                    }
                }
                // TODO: disks in a computer sit at the computer type's drive offset (item type 0x27 +0x11c8, +0x11cc)
                ItemKind::Computer => p,
                _ => Vec3::new(p.x + r1.x * BELOW, r1.y * BELOW + p.y, r1.z * BELOW + p.z),
            };
            let item = self.items.get_mut(id).unwrap();
            item.mount_pose = Some((pos, rot));
            item.pos2 = pos;
        }
    }

    fn snap_pocketed_items(&mut self) {
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

    /// The soccer ball's part of item_simulation: item_soccerball_kick, then the ball against the level
    /// (sphere_cast_level) with its spin damped while it touches.
    fn soccer_ball(&mut self, id: usize, body_id: usize) {
        self.soccerball_kick(id, body_id);
        let Some(center) = self.bodies.get(body_id).map(|b| b.pos) else { return };
        let level = &self.world.map.level;
        let Some((p, n, dist)) = crate::world::sphere_cast::sphere_cast_level(&level.area, &level.meshes, center, BALL_RADIUS) else { return };
        let offset = Vec3::new(p.x - center.x, p.y - center.y, p.z - center.z);
        self.bodies.add_world_contact(body_id, offset, n, BALL_RADIUS - dist, CONTACT_FRICTION, CONTACT_DEPTH_SCALE, CONTACT_SOFTNESS);
        if let Some(b) = self.bodies.get_mut(body_id) {
            let l = b.ang_momentum;
            b.ang_momentum = Vec3::new(l.x * BALL_SPIN_DAMPING, l.y * BALL_SPIN_DAMPING, l.z * BALL_SPIN_DAMPING);
        }
    }

    /// item_soccerball_kick: every bone but the arms' within reach of the ball pushes it away from the bone, with a
    /// contact on the ball's surface facing the bone.
    fn soccerball_kick(&mut self, id: usize, ball_body: usize) {
        let Some(ball) = self.items.get(id).map(|i| i.pos2) else { return };
        let mut contacts = Vec::new();
        for (_, h) in self.humans.iter() {
            for (k, bone) in h.bones.iter().enumerate() {
                if KICK_SKIPPED_BONES.contains(&k) {
                    continue;
                }
                let d = Vec3::new(ball.x - bone.pos.x, ball.y - bone.pos.y, ball.z - bone.pos.z);
                let len = (d.z * d.z + (d.x * d.x + d.y * d.y)).sqrt();
                if !(KICK_REACH > len) {
                    continue;
                }
                let (off, n) = if len != 0.0 {
                    let inv = 1.0 / len;
                    let u = Vec3::new(d.x * inv, d.y * inv, inv * d.z);
                    (Vec3::new(u.x * -BALL_RADIUS, u.y * -BALL_RADIUS, u.z * -BALL_RADIUS), Vec3::new(-u.x, -u.y, -u.z))
                } else {
                    (Vec3::splat(-0.0), Vec3::splat(-0.0))
                };
                let p = Vec3::new(off.x + ball.x, off.y + ball.y, off.z + ball.z);
                contacts.push((bone.body, p, n, KICK_REACH - len));
            }
        }
        for (bone_body, p, n, depth) in contacts {
            let (Some(a), Some(b)) = (self.bodies.get(bone_body).map(|b| b.pos), self.bodies.get(ball_body).map(|b| b.pos)) else { continue };
            let (off_a, off_b) = (Vec3::new(p.x - a.x, p.y - a.y, p.z - a.z), Vec3::new(p.x - b.x, p.y - b.y, p.z - b.z));
            self.bodies.add_body_contact(bone_body, ball_body, off_a, off_b, n, depth, KICK_FRICTION, KICK_DEPTH_SCALE, KICK_SOFTNESS);
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
                let tail = match (&item.state, item.item_type) {
                    (ItemState::Cash(c), _) => ItemTail::Cash { bills: c.bills, spread: c.spread, codes: c.codes },
                    (ItemState::Radio { transmitting, .. }, _) => ItemTail::Radio(*transmitting),
                    (_, ItemKind::CashWorld) => ItemTail::Cash { bills: 0, spread: 0, codes: 0 },
                    (_, ItemKind::Radio) => ItemTail::Radio(false),
                    // TODO: the computer's cursor (item +0x370) once computers are ported
                    (_, ItemKind::Computer) => ItemTail::Computer(0),
                    _ => ItemTail::None,
                };
                Some(ServerItemObject { slot: (HUMAN_SLOTS + id) as u16, item_id: id as u16, item_type: item.item_type, pos: Vector(body.pos), rot, parent_item: item.parent_item, parent_human: item.parent_human, parent_slot: item.parent_slot, tail })
            })
            .collect()
    }
}

/// item_attach_child: mounts `child` on `parent` if its type mounts there; a parent takes one item, or up to five
/// cash bundles, and at most eight in all.
pub fn attach_child(items: &mut Table<Item>, types: &[ItemType], parent: usize, child: usize) -> bool {
    let (Some(p), Some(c)) = (items.get(parent), items.get(child)) else { return false };
    let n = p.children.len();
    if n > 7 || types[c.item_type as usize].can_mount_to[p.item_type as usize] == 0 {
        return false;
    }
    let room = if matches!(c.item_type, ItemKind::CashRound | ItemKind::CashWorld) { n <= 4 } else { n == 0 };
    if !room {
        return false;
    }
    let p = items.get_mut(parent).unwrap();
    p.children.push(child);
    p.first_child = p.children[0];
    let c = items.get_mut(child).unwrap();
    c.parent_item = parent as i32;
    c.parent_slot = n as i32;
    true
}

/// item_removelink: takes `item` off `parent`, the last child taking its place.
pub fn remove_link(items: &mut Table<Item>, item: usize, parent: usize) {
    if let Some(p) = items.get_mut(parent) {
        while let Some(i) = p.children.iter().position(|&c| c == item) {
            p.children.swap_remove(i);
            if let Some(&c) = p.children.first() {
                p.first_child = c;
            }
        }
    }
    if let Some(c) = items.get_mut(item) {
        c.parent_item = -1;
    }
}
