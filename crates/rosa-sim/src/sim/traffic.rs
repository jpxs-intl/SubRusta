use glam::Vec3;
use rosa_math::vector::Vector;
use rosa_protocol::{
    GameMode,
    clientbound::game::{TrafficEntry, events::{Event, ServerEvent, update_vehicle_type_color::EventUpdateVehicleTypeColor}},
};

use super::Sim;
use crate::{
    traffic::{DRIVEN, LOOP_STREET, LOOP_STREET_2, TAKEN, Traffic, ai::ai_traffic_car, grid::add_cars, motion::{move_virtual_car, update_signals}, rand_bit, rand_mod, route::plan_route, spawn::create_traffic},
    vehicle::spawn_vehicle,
};

/// reset_game makes this many cars when the map has streets.
const TRAFFIC_CARS: i32 = 128;
/// A car becomes a real vehicle within 96 of a player (16 of a bot), and stays one 4 further out.
const PLAYER_RANGE: f32 = 96.0;
const KEEP_RANGE: f32 = 4.0;
const NEAR: i32 = 2;
const EDGE: i32 = 1;
/// The vehicle sits 1.5 back from the car's position along its forward axis.
const BODY_OFFSET: f32 = 1.5;
/// A driven vehicle's gas: the speed error times 8 (doubled braking), less with more steering, and full reverse below
/// 0.05.
const GAS_GAIN: f32 = 8.0;
const CREEP: f32 = f32::from_bits(0x3d4c_cccd);
const DRIVE_GEAR: f32 = 2.0;
const REVERSE_PULL: f32 = 1.0;
const STUCK_TICKS: i32 = 179;
const STUCK_REVERSE: i32 = 279;

/// Where a traffic car's vehicle goes: its type's chassis centred on the car, 1.5 back from its position.
pub(crate) fn vehicle_pos(c: &crate::traffic::TrafficCar, types: &[crate::vehicle::types::VehicleType]) -> Vec3 {
    let [r0, r1, r2] = c.rot;
    let k = types.get(c.kind).map_or(Vec3::ZERO, |t| t.centroid);
    let (nx, ny, nz) = (-k.x, -k.y, -k.z);
    let back = -BODY_OFFSET;
    let p = c.pos;
    Vec3::new(
        (((back * r2.x + p.x) + nx * r0.x) + ny * r1.x) + r2.x * nz,
        r2.y * nz + (((r2.y * back + p.y) + r0.y * nx) + r1.y * ny),
        nz * r2.z + (((r2.z * back + p.z) + r0.z * nx) + r1.z * ny),
    )
}

impl Sim {
    /// The shop and traffic part of reset_game: srand(time), the dealerships and gun stores restocked, and 128 cars when
    /// the map has streets (none for races).
    pub(crate) fn reset_game(&mut self) {
        for (_, p) in self.players.iter_mut() {
            p.items_bought = 0;
            p.actions.reset();
        }
        let map = &self.world.map;
        self.traffic = Traffic::new(&map.streets, map.map_name == "round");
        self.missions.counter = 0;
        // TODO: versus reads its share from config_versus.txt (0x4538562c)
        self.team_damage = match self.gamemode {
            GameMode::Round => self.round_cfg.teamdamage,
            GameMode::Versus => crate::sim::crime::VERSUS_TEAM_DAMAGE,
            _ => 0,
        };
        for c in &mut self.corp_state {
            for m in &mut c.missions {
                m.active = false;
                m.unk_5c = 0;
            }
        }
        if self.gamemode == GameMode::Round {
            self.reset_round();
        }
        if self.gamemode == GameMode::Eliminator {
            self.reset_eliminator();
        }
        if self.gamemode == GameMode::Sandbox {
            self.gamestate = rosa_protocol::clientbound::game::GameState::InGame;
        }
        self.traffic.coop = self.gamemode == GameMode::CoOp;
        if self.gamemode != GameMode::World {
            self.roll_weather();
        }
        crate::rng::srand(crate::rng::time_seed());
        self.restock_dealerships();
        self.stock_gun_stores();
        let map = &self.world.map;
        if !matches!(self.gamemode, GameMode::Racing | GameMode::Round | GameMode::Eliminator) && !map.streets.streets.is_empty() {
            create_traffic(&mut self.traffic, map, &self.vehicle_types, self.gamemode, TRAFFIC_CARS);
        }
        // TODO: round and eliminator modes work the doors from logic_round and logic_eliminator once those are ported
        if self.gamemode == GameMode::World {
            for k in 0..crate::sim::corporations::CORPORATIONS {
                self.set_team_door(k, true);
            }
            self.apply_team_doors();
        }
        self.announce_players();
    }

    /// The end of reset_game: the share prices and every player again, since the event list starts over.
    fn announce_players(&mut self) {
        let e = crate::sim::economy::stock_event(&self.corporations, self.tick);
        self.events.push(e);
        let tick = self.tick;
        let es: Vec<_> = self.players.iter().flat_map(|(_, p)| [p.make_update_player_event(tick), p.make_update_round_event(tick)]).collect();
        for e in es {
            self.events.push(e);
        }
    }

    /// simulate_traffic: the lights, the street grid, the AI, which cars are near enough a player to be real
    /// vehicles, and the movement of the rest.
    pub(crate) fn simulate_traffic(&mut self) {
        self.take_over_traffic();
        update_signals(&mut self.traffic, &self.world.map.streets);
        let map = &self.world.map;
        add_cars(&mut self.traffic, &map.streets);
        for id in 0..self.traffic.cars.len() {
            if self.traffic.cars[id].is_bot == DRIVEN {
                ai_traffic_car(&mut self.traffic, &map.streets, id);
            }
        }
        for c in &mut self.traffic.cars {
            if let Some(v) = usize::try_from(c.vehicle).ok().and_then(|v| self.vehicles.get(v)) {
                c.pos = v.pos;
            }
        }
        self.traffic_states();
        self.traffic_spawns();
        for id in 0..self.traffic.cars.len() {
            if self.traffic.cars[id].vehicle == -1 {
                move_virtual_car(&mut self.traffic, &self.world.map, &self.vehicle_types, id);
            } else {
                self.drive_traffic_vehicle(id);
            }
        }
    }

    /// The vehicles a player drove off with free hands this tick: their cars stop driving themselves.
    fn take_over_traffic(&mut self) {
        for (_, v) in self.vehicles.iter_mut() {
            if std::mem::take(&mut v.traffic_taken)
                && let Some(c) = usize::try_from(v.traffic_car).ok().and_then(|t| self.traffic.cars.get_mut(t))
            {
                c.is_bot = TAKEN;
            }
        }
    }

    /// How near the nearest living player's human is to each car.
    fn traffic_states(&mut self) {
        // TODO: bots' humans count within 16 rather than 96 (player +0x2d18)
        for c in &mut self.traffic.cars {
            c.state = 0;
            for (hid, h) in self.humans.iter() {
                if h.player.is_none() || h.old_health <= 0 || c.state > EDGE || c.human == hid as i32 {
                    continue;
                }
                let b = h.bones[0].pos;
                let (dx, dy, dz) = (c.pos.x - b.x, c.pos.y - b.y, c.pos.z - b.z);
                let dist = (dz * dz + (dx * dx + dy * dy)).sqrt();
                if PLAYER_RANGE > dist {
                    c.state = NEAR;
                } else if PLAYER_RANGE + KEEP_RANGE > dist {
                    c.state = EDGE;
                }
            }
            if self.traffic.roundcity && (c.physical_street == LOOP_STREET || c.physical_street == LOOP_STREET_2) {
                c.state = 0;
            }
        }
    }

    /// Cars near a player become vehicles; cars away from every player lose theirs (a taken car is handed back to the
    /// traffic first).
    fn traffic_spawns(&mut self) {
        let n_streets = self.world.map.streets.streets.len() as i32;
        for id in 0..self.traffic.cars.len() {
            let c = &self.traffic.cars[id];
            if c.state == NEAR {
                if c.vehicle != -1 {
                    continue;
                }
                let pos = vehicle_pos(c, &self.vehicle_types);
                let (vel, rot, kind, color) = (c.vel, c.rot, c.kind, c.color);
                let vid = spawn_vehicle(&mut self.vehicles, &mut self.bodies, &self.vehicle_types, kind, color, pos, rot, Some(vel));
                self.traffic.cars[id].vehicle = vid.map_or(-1, |v| v as i32);
                if let Some(v) = vid {
                    if let Some(veh) = self.vehicles.get_mut(v) {
                        veh.traffic_car = id as i32;
                    }
                    let e = EventUpdateVehicleTypeColor { vehicle_id: v as i32, vehicle_type: kind as u8, vehicle_color: color as u8 };
                    self.events.push(Event { tick_created: self.tick, kind: ServerEvent::UpdateVehicleTypeColor(e) });
                }
                continue;
            }
            if c.state != 0 || c.is_bot <= 0 {
                continue;
            }
            if c.is_bot == TAKEN {
                if c.physical_street != -1 {
                    let ps = c.physical_street;
                    let to = rand_mod(n_streets);
                    let r = rand_bit();
                    let c = &mut self.traffic.cars[id];
                    c.street = ps;
                    c.intersection = -1;
                    plan_route(c, &self.world.map.streets, ps, 0, to, r);
                    c.is_aggressive = 0;
                    c.is_bot = DRIVEN;
                } else if c.is_aggressive != 0 {
                    continue;
                }
            } else if c.is_aggressive != 0 {
                continue;
            }
            let c = &self.traffic.cars[id];
            if let Some(v) = usize::try_from(c.vehicle).ok().and_then(|v| self.vehicles.get_mut(v)) {
                v.despawn_time = 0;
            }
            if let Some(h) = usize::try_from(c.human).ok().and_then(|h| self.humans.get_mut(h)) {
                h.despawn_ticks = 0;
            }
        }
    }

    /// A car near a player follows its vehicle, and drives it while the traffic does: second gear, the AI's steering,
    /// gas from the speed error, and backing out when stuck.
    fn drive_traffic_vehicle(&mut self, id: usize) {
        let c = &mut self.traffic.cars[id];
        let Some(v) = usize::try_from(c.vehicle).ok().and_then(|v| self.vehicles.get_mut(v)) else { return };
        let k = self.vehicle_types.get(c.kind).map_or(Vec3::ZERO, |t| t.centroid);
        let [r0, r1, r2] = v.rot;
        let vp = v.pos;
        c.vel = v.vel;
        c.rot = v.rot;
        c.pos = Vec3::new(
            (((r2.x * BODY_OFFSET + vp.x) + r0.x * k.x) + r1.x * k.y) + k.z * r2.x,
            k.z * r2.y + (r1.y * k.y + (r0.y * k.x + (vp.y + r2.y * BODY_OFFSET))),
            k.z * r2.z + (k.y * r1.z + (k.x * r0.z + (BODY_OFFSET * r2.z + vp.z))),
        );
        c.yaw = ((-r2.z) as f64).atan2((-r2.x) as f64) as f32;
        if c.is_bot != DRIVEN {
            return;
        }
        v.gear_x = DRIVE_GEAR;
        v.gear_y = -1.0;
        v.steer_control = c.steer;
        let mut g = (((v.vel.x * r2.x + v.vel.y * r2.y) + v.vel.z * r2.z) + c.target_speed) * GAS_GAIN;
        if 0.0 > g {
            g = g + g;
        }
        v.gas_control = g;
        let gd = if -1.0 > g {
            v.gas_control = -1.0;
            -1.0
        } else if g > 1.0 {
            v.gas_control = 1.0;
            1.0
        } else {
            g as f64
        };
        v.gas_control = if CREEP > c.target_speed { -1.0 } else { (gd / ((c.steer.abs() as f64) + 1.0)) as f32 };
        v.controls = 0;
        if c.stuck > STUCK_TICKS {
            v.steer_control = 0.0;
            v.gear_x = DRIVE_GEAR;
            v.gear_y = REVERSE_PULL;
            v.gas_control = if c.stuck > STUCK_REVERSE { -1.0 } else { 1.0 };
        }
    }
}

/// Each car's priority grows by up to 256 a tick, more the nearer it is to the camera, the less it steers and the
/// faster it goes (at least 16); the client gets the 16 most overdue each packet.
const PRIORITY_SCALE: f32 = 256.0;
const PRIORITY_FALLOFF: f32 = 0.001953125;
const PRIORITY_MIN: i32 = 16;
const STEER_WEIGHT: f64 = 4.0;
const MOVING_WEIGHT: f32 = 5.0;
const MOVING: f32 = f32::from_bits(0x3c88_8889);
const SENT_PER_PACKET: usize = 16;

/// The traffic part of a client's game packet: the most overdue cars and one intersection's lights.
pub(crate) fn traffic_section(client: &mut crate::Client, traffic: &Traffic, map: &crate::world::streets::StreetMap, camera: Vec3) -> (Vec<TrafficEntry>, (i32, [i32; 4])) {
    // TODO: 32 cars a packet and a priority of 1 when the connection's flag at [rsp+8] is 0xff
    let n = traffic.cars.len();
    client.traffic_priority.resize(n, 0);
    for (c, p) in traffic.cars.iter().zip(client.traffic_priority.iter_mut()) {
        let d = Vec3::new(camera.x - c.pos.x, camera.y - c.pos.y, camera.z - c.pos.z);
        let dist = ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt();
        let near = (dist as f64 / ((c.steer.abs() as f64) * STEER_WEIGHT + 1.0)) as f32;
        let speed = c.speed();
        let k = if MOVING <= speed { MOVING_WEIGHT } else { speed * 60.0 * 4.0 + 1.0 };
        *p += (((1.0 - (near / k) * PRIORITY_FALLOFF) * PRIORITY_SCALE) as i32).max(PRIORITY_MIN);
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| client.traffic_priority[i]);
    let mut sent: Vec<usize> = order.into_iter().rev().take(SENT_PER_PACKET).collect();
    for &i in &sent {
        client.traffic_priority[i] = 0;
    }
    sent.sort_unstable();
    let entries = sent
        .into_iter()
        .map(|i| {
            let c = &traffic.cars[i];
            TrafficEntry { index: i as u16, vehicle: c.vehicle != -1, kind: c.kind as i32, color: c.color, pos: Vector(c.pos), yaw: c.yaw }
        })
        .collect();
    let count = map.intersections.len() as i32;
    let at = if count > 0 { client.signal_cursor.rem_euclid(count) } else { 0 };
    client.signal_cursor = client.signal_cursor.wrapping_add(1);
    let lights = traffic.signals.get(at as usize).map_or([0; 4], |s| [s.lights[0], s.lights[1], s.lights[2], s.lights[3]]);
    (entries, (at, lights))
}
