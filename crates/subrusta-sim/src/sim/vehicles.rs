use glam::Vec3;
use subrusta_math::vector::Vector;
use subrusta_physics::rotation::{IDENTITY, rot_matrix_to_quaternion, rotate_orientation};
use subrusta_protocol::clientbound::game::{
    ServerVehicleObject, VehicleKind,
    events::{Event, ServerEvent, update_vehicle::EventUpdateVehicle, update_vehicle_type_color::EventUpdateVehicleTypeColor},
};

use super::Sim;
use crate::{
    PlayerId,
    items::state::ItemState,
    vehicle::{NETWORKED_VEHICLES, physics, spawn_vehicle},
    world::trace::line_intersect_level,
};

const SPAWN_DISTANCE: f32 = 6.0;
const GROUND_SEARCH: f32 = 8.0;
const RIDE_HEIGHT: f32 = 1.0;
const HIDDEN_STATE: i32 = 3;
/// The update_vehicle kinds: a window broken, a tyre burst, the vehicle wrecked.
pub(crate) const WINDOW_BROKEN: i32 = 0;
pub(crate) const TYRE_BURST: i32 = 1;
const WRECKED: i32 = 2;
const DELETED: i32 = 3;
/// A despawn time above this never runs out.
const NEVER_DESPAWNS: i32 = 65534;
const ITEM_FRICTION: f32 = 0.4;
const ITEM_DEPTH_SCALE: f32 = 1.0 / 64.0;
const ITEM_SOFTNESS: f32 = 1.0 / 32.0;
const NETWORKED_WHEELS: usize = 4;

/// Each vehicle's priority grows by up to 256 a packet, more the nearer it is to the camera, the less it steers and the
/// faster it goes (at least 32), and by 1024 within 4 of the camera (12 for a train); the client gets the 16 most
/// overdue, the first two at tier 0 and the next six at tier 1 (both starting over), the rest at tier 2, and only the
/// first two while it is more than 22 events behind.
const PRIORITY_SCALE: f32 = 256.0;
const PRIORITY_FALLOFF: f32 = 1.0 / 256.0;
const PRIORITY_MIN: i32 = 32;
const STEER_WEIGHT: f64 = 4.0;
const MOVING: f32 = 1.0 / 60.0;
const MOVING_WEIGHT: f32 = 5.0;
const CLOSE: f32 = 4.0;
const TRAIN_CLOSE: f32 = 12.0;
const CLOSE_PRIORITY: i32 = 1024;
const SENT_PER_PACKET: usize = 16;
const TIER_0: usize = 2;
const TIER_1: usize = 8;
const BACKLOG: u16 = 22;
const BACKLOGGED_SENT: usize = 2;

/// What a vehicle's priority is worked out from: its position, steering input, velocity and whether it is a train.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VehicleSight {
    pos: Vec3,
    steer_control: f32,
    vel: Vec3,
    train: bool,
}

/// The vehicle part of append_object_packet for one connection: every vehicle's priority grown, then the most overdue
/// sent with their tiers.
pub(crate) fn vehicle_section(
    priority: &mut Vec<i32>,
    vehicles: &[(ServerVehicleObject, VehicleSight)],
    camera: Vec3,
    backlog: u16,
) -> Vec<ServerVehicleObject> {
    priority.resize(NETWORKED_VEHICLES, 0);
    for (o, s) in vehicles {
        let d = camera - s.pos;
        let dist = d.length();
        let near = (dist as f64 / ((s.steer_control.abs() as f64) * STEER_WEIGHT + 1.0)) as f32;
        let speed = s.vel.length();
        let k = if MOVING <= speed { MOVING_WEIGHT } else { speed * 60.0 * 4.0 + 1.0 };
        let mut p = (((1.0 - (near / k) * PRIORITY_FALLOFF) * PRIORITY_SCALE) as i32).max(PRIORITY_MIN);
        if dist < CLOSE || (s.train && dist < TRAIN_CLOSE) {
            p = CLOSE_PRIORITY;
        }
        priority[o.vehicle_id as usize] += p;
    }
    let mut order: Vec<usize> = (0..vehicles.len()).collect();
    order.sort_by_key(|&i| priority[vehicles[i].0.vehicle_id as usize]);
    let sent: Vec<ServerVehicleObject> = order
        .into_iter()
        .rev()
        .take(SENT_PER_PACKET)
        .enumerate()
        .map(|(rank, i)| {
            let o = &vehicles[i].0;
            let tier = if rank < TIER_0 {
                0
            } else if rank < TIER_1 {
                1
            } else {
                2
            };
            if tier < 2 {
                priority[o.vehicle_id as usize] = 0;
            }
            ServerVehicleObject { tier, ..o.clone() }
        })
        .collect();
    let cap = if backlog > BACKLOG { BACKLOGGED_SENT } else { SENT_PER_PACKET };
    sent.into_iter().take(cap).collect()
}
const BYTE: f64 = 255.0;

fn to_byte(v: f32, min: i32) -> i32 {
    ((v * 255.0) as i32).clamp(min, 255)
}

impl Sim {
    /// spawn_vehicle and its update_vehicle_type_and_color event.
    pub(crate) fn spawn_vehicle(
        &mut self,
        kind: subrusta_protocol::clientbound::game::VehicleKind,
        color: i32,
        pos: Vec3,
        rot: subrusta_physics::RotMatrix,
    ) -> Option<usize> {
        let id = spawn_vehicle(&mut self.vehicles, &mut self.bodies, &self.vehicle_types, kind, color, pos, rot, None)?;
        let e = EventUpdateVehicleTypeColor { vehicle_id: id as i32, vehicle_type: kind, vehicle_color: color as u8 };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::UpdateVehicleTypeColor(e) });
        Some(id)
    }

    /// Test command (/car [type] [color]): a vehicle in front of the player, facing the way they look.
    pub(crate) fn car_command(&mut self, player_id: PlayerId, message: &str) {
        let mut args = message.split_whitespace().skip(1).map(|a| a.parse::<i32>().ok());
        let kind = args.next().flatten().unwrap_or(0);
        let color = args.next().flatten().unwrap_or(0);
        let kind = u8::try_from(kind)
            .ok()
            .and_then(|k| subrusta_protocol::clientbound::game::VehicleKind::try_from(k).ok())
            .filter(|&k| self.vehicle_types.get(k as usize).is_some_and(|t| !t.nodes.is_empty()));
        let Some(kind) = kind else {
            let ready: Vec<String> =
                self.vehicle_types.iter().enumerate().filter(|(_, t)| !t.nodes.is_empty()).map(|(k, t)| format!("{k} {}", t.name)).collect();
            self.send_chat(&format!("Vehicle types: {}", ready.join(", ")), subrusta_protocol::clientbound::game::events::chat::ChatType::Announce, -1, 0);
            return;
        };
        let Some(player) = self.players.get(player_id.idx()) else { return };
        let mut rot = IDENTITY;
        rotate_orientation(&mut rot, Vec3::Y, player.view_yaw);
        let ahead = player.camera_pos.0 - rot[0] * SPAWN_DISTANCE;
        let map = &self.world.map;
        let ground = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, ahead + Vec3::Y * GROUND_SEARCH, ahead - Vec3::Y * GROUND_SEARCH);
        let pos = ground.map_or(ahead, |g| Vec3::new(ahead.x, g.hit.pos.y + RIDE_HEIGHT, ahead.z));
        match self.spawn_vehicle(kind, color, pos, rot) {
            Some(id) => println!("[Sim] Spawned vehicle {kind:?} #{id} at {pos:?}"),
            None => println!("[Sim] Vehicle table full"),
        }
    }

    /// The vehicle part of build_objectpacket: each vehicle's position, orientation, steering, wheels and engine, and
    /// what its priority for each connection is worked out from.
    pub(crate) fn vehicle_objects(&self) -> Vec<(ServerVehicleObject, VehicleSight)> {
        self.vehicles
            .iter()
            .filter(|(id, v)| *id < NETWORKED_VEHICLES && v.controllable_state != HIDDEN_STATE)
            .map(|(id, v)| {
                let steer = ((v.steer as f64 / 180.0_f64.to_radians() * BYTE) as i32).clamp(-255, 255);
                let wheels = std::array::from_fn(|k| {
                    v.wheels
                        .get(k)
                        .filter(|_| k < NETWORKED_WHEELS)
                        .map_or([0; 3], |w| [to_byte(w.visual_height, 0), to_byte(w.spin, -255), to_byte(w.skid, 0)])
                });
                let object = ServerVehicleObject {
                    vehicle_id: id as u16,
                    traffic_car: v.traffic_car,
                    pos: Vector(v.pos),
                    rot: rot_matrix_to_quaternion(&v.rot),
                    steer,
                    wheels,
                    engine_rpm: v.engine_rpm,
                    tier: 0,
                };
                (
                    object,
                    VehicleSight {
                        pos: v.pos,
                        steer_control: v.steer_control,
                        vel: v.vel,
                        train: v.kind == subrusta_protocol::clientbound::game::VehicleKind::Train,
                    },
                )
            })
            .collect()
    }
}

impl Sim {
    /// vehicle_update_bbox_wake_items, after the bodies move: each vehicle's record, then the set items in the
    /// blocks its bounds cover.
    pub(crate) fn update_vehicle_bounds(&mut self) {
        for id in self.vehicles.ids() {
            let Some(v) = self.vehicles.get_mut(id) else { continue };
            if self.bodies.get(v.body).is_none() {
                continue;
            }

            physics::update_vehicle_bounds(v, &self.bodies, &self.vehicle_types);
            let (lo, hi) = (glam::IVec3::from_array(v.block_min), glam::IVec3::from_array(v.block_max));

            self.spawn_set_items_in(lo, hi);
        }
    }

    /// vehicleSimulation, with its crash sounds and broken glass as events.
    pub(crate) fn simulate_vehicles(&mut self) {
        use crate::{human::physics::HumanOutput, vehicle::physics::VehicleOutput};
        let driven: Vec<usize> = self.humans.iter().filter(|(_, h)| h.seat == 0).filter_map(|(_, h)| h.vehicle).collect();
        let (out, glass) = physics::vehicle_simulation(&mut self.vehicles, &mut self.bodies, &mut self.world.map, &self.vehicle_types, &driven);

        self.vehicle_item_contacts();

        physics::vehicle_vehicle_contacts(&self.vehicles, &mut self.bodies, &self.vehicle_types);

        for o in out {
            match o {
                VehicleOutput::Sound { sound, pos, volume, pitch } => self.apply_human_outputs(vec![HumanOutput::Sound { sound, pos, volume, pitch }]),
                VehicleOutput::Damage { vehicle, amount } => self.vehicle_take_damage(vehicle, amount),
                VehicleOutput::Update { vehicle, kind, part, pos, vel } => {
                    let e = EventUpdateVehicle { vehicle_id: vehicle as i32, kind, part, pos: Vector(pos), velocity: Vector(vel) };
                    self.events.push(Event { tick_created: self.tick, kind: ServerEvent::UpdateVehicle(e) });
                }
            }
        }

        self.apply_human_outputs(glass);
    }

    /// vehicle_take_damage: a vehicle's health worn down; at nothing it is wrecked (update_vehicle kind 2). Trains
    /// cannot be hurt.
    pub(crate) fn vehicle_take_damage(&mut self, id: usize, amount: i32) {
        let Some(v) = self.vehicles.get_mut(id) else { return };
        if v.kind == VehicleKind::Train || v.health <= 0 {
            return;
        }

        v.health -= amount;

        if v.health <= 0 {
            let e = EventUpdateVehicle { vehicle_id: id as i32, kind: WRECKED, part: 0, pos: Vector(v.pos), velocity: Vector(v.vel) };
            self.events.push(Event { tick_created: self.tick, kind: ServerEvent::UpdateVehicle(e) });
        }
    }

    /// delete_vehicle: anyone inside is let go, the chassis and wheel bodies removed and the clients told (update_vehicle
    /// kind 3).
    pub(crate) fn delete_vehicle(&mut self, id: usize) {
        for (_, h) in self.humans.iter_mut() {
            if h.vehicle == Some(id) {
                h.vehicle = None;
            }
        }
        for c in &mut self.traffic.cars {
            if c.vehicle == id as i32 {
                c.vehicle = -1;
            }
        }
        for (_, item) in self.items.iter_mut() {
            if let ItemState::Key { vehicle } = &mut item.state
                && *vehicle == Some(id)
            {
                item.despawn_time = 0;
                *vehicle = None;
            }
        }
        let Some(v) = self.vehicles.remove(id) else { return };
        for w in &v.wheels {
            self.bodies.remove(w.body);
        }
        if let Some(r) = &v.rotor {
            self.bodies.remove_bond(r.point_bond);
            self.bodies.remove_bond(r.angular_bond);
            self.bodies.remove(r.body);
        }
        for &(bogie, turn) in &v.bogies {
            self.bodies.remove_bond(turn);
            self.bodies.remove(bogie);
        }
        self.bodies.remove(v.body);
        let e = EventUpdateVehicle { vehicle_id: id as i32, kind: DELETED, part: 0, pos: Vector(v.pos), velocity: Vector(Vec3::ZERO) };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::UpdateVehicle(e) });
    }

    /// cleanup_vehicles: each vehicle's despawn time runs down and the vehicle is deleted when it runs out.
    pub(crate) fn cleanup_vehicles(&mut self) {
        for id in self.vehicles.ids() {
            let Some(v) = self.vehicles.get_mut(id) else { continue };
            let t = v.despawn_time as i32;
            if t > NEVER_DESPAWNS {
                continue;
            }
            v.despawn_time = (t - 1) as u16;
            if t - 1 <= 0 {
                self.delete_vehicle(id);
            }
        }
    }

    /// Test command (/clear): every item, vehicle, bullet and human without a player is removed.
    pub(crate) fn clear_command(&mut self) {
        for id in self.items.ids() {
            self.delete_item(id);
        }
        for id in self.vehicles.ids() {
            self.delete_vehicle(id);
        }
        for id in self.humans.ids() {
            if self.humans.get(id).is_some_and(|h| h.player.is_none()) {
                self.delete_human(id);
            }
        }
        self.bullets.clear();
        self.send_chat("Cleared", subrusta_protocol::clientbound::game::events::chat::ChatType::Announce, -1, 0);
    }

    /// The item pass at the end of vehicleSimulation: the chassis hull of each vehicle against the hulls of the items
    /// in its blocks whose bounds overlap it, waking the items it touches.
    fn vehicle_item_contacts(&mut self) {
        for (_, v) in self.vehicles.iter() {
            let Some(hull) = self.vehicle_types.get(v.kind as usize).and_then(|t| t.mesh.as_ref()) else { continue };
            let found = self.item_grid.query(glam::IVec3::from_array(v.block_min), glam::IVec3::from_array(v.block_max));
            for id in found {
                let Some(item) = self.items.get(id) else { continue };
                let (mn, mx) = (item.aabb_min, item.aabb_max);
                if mn.x > v.bounds_max.x
                    || v.bounds_min.x > mx.x
                    || mn.z > v.bounds_max.z
                    || v.bounds_min.z > mx.z
                    || mn.y > v.bounds_max.y
                    || v.bounds_min.y > mx.y
                {
                    continue;
                }
                let Some(item_hull) = &self.item_types[item.item_type as usize].hull else { continue };
                let item_body = item.body;
                let Some((ipos, irot)) = self.bodies.get(item_body).map(|b| (b.pos, b.rot)) else { continue };
                let contacts = super::hull::collide_convex_hulls(hull, v.pos, &v.rot, item_hull, ipos, &irot);
                for c in &contacts {
                    let (first, second) = if c.swapped { (item_body, v.body) } else { (v.body, item_body) };
                    self.bodies.add_body_contact(first, second, c.offset_a, c.offset_b, c.normal, c.depth, ITEM_FRICTION, ITEM_DEPTH_SCALE, ITEM_SOFTNESS);
                }
                if !contacts.is_empty()
                    && let Some(item) = self.items.get_mut(id)
                {
                    item.physics_settled = false;
                    item.settled_timer = 0;
                }
            }
        }
    }

    /// bond_simulation with the wheels' constraint and drive steps in each solver pass.
    pub(crate) fn solve_bonds(&mut self) {
        use subrusta_physics::body::SolverStep;
        let Sim { bodies, vehicles, .. } = self;
        bodies.solve_bonds_with(|b, step| match step {
            SolverStep::BeforeBonds => crate::vehicle::physics::step_wheel_constraints(vehicles, b),
            SolverStep::AfterBonds => crate::vehicle::physics::apply_wheel_forces(vehicles, b),
        });
    }
}
