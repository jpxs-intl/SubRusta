use glam::Vec3;
use rosa_math::vector::Vector;
use rosa_physics::{
    RigidBodies, RotMatrix,
    rotation::{IDENTITY, rot_matrix_to_quaternion, rotate_orientation},
};
use rosa_protocol::{GameMode, clientbound::game::{ItemKind, OwnHumanData, ServerHumanObject, events::{Event, ServerEvent, bullet_hit::EventBulletHit, sound::EventSound, bullet_hole::EventBulletHole}}};

use super::{Sim, economy, items::Touchables};
use crate::{
    PlayerId,
    human::{
        Human,
        create::create_human,
        inventory::detach_item,
        physics::{HumanOutput, sync_bones},
        simulation::{HumanTick, simulate_human},
    },
};

const SPAWN_DISTANCE: f32 = 2.0;
/// The versus start delay is configured in seconds.
const TICKS_PER_SECOND: i32 = 60;
pub const HUMAN_SLOTS: usize = crate::human::MAX_HUMANS;
pub const OBJECT_SLOTS: usize = 1024;

impl Sim {
    pub fn spawn_human(&mut self, pos: Vec3, rot: &RotMatrix, player: Option<PlayerId>) -> Option<usize> {
        let player = player.and_then(|p| self.players.get(p.idx()));
        create_human(&mut self.humans, &mut self.bodies, pos, rot, player)
    }

    pub(crate) fn spawn_human_for(&mut self, player_id: PlayerId) {
        let Some(player) = self.players.get(player_id.idx()) else { return };

        let pos = player.camera_pos.0;
        let Some(id) = self.spawn_human(pos, &IDENTITY, Some(player_id)) else {
            return println!("[Sim] Human table full");
        };
        if let Some(player) = self.players.get_mut(player_id.idx()) {
            player.human = Some(id);
            player.ghost_human = false;
            player.menu = rosa_protocol::clientbound::game::MenuType::Empty;
            player.items_bought = 0;
            self.events.push(player.make_update_player_event(self.tick));
        }
        println!("[Sim] Spawned human #{id} at {:?}", pos);
    }

    /// player_simulation: hands each player's latest controls to their human.
    pub(crate) fn player_simulation(&mut self) {
        let held = self.gamemode == GameMode::Versus && self.round_elapsed < self.versus_cfg.movedelay * TICKS_PER_SECOND;
        for (_, player) in self.players.iter_mut() {
            if player.spawn_timer > 0 {
                player.spawn_timer -= 1;
            }
            // TODO: the rest of the per-player bookkeeping (account and game mode timers)
            let Some(h) = player.human.and_then(|id| self.humans.get_mut(id)) else { continue };
            h.stocks = player.stocks;
            if h.vehicle.is_some() {
                if matches!(player.control_mode, 2 | 3) {
                    let c = &player.controls;
                    h.gear_x_input = c[0];
                    h.strafe_input = c[1];
                    h.gear_y_input = c[2];
                    h.walk_input = c[3];
                    h.look_yaw = c[4];
                    h.look_pitch = c[5];
                    h.free_look_yaw = c[6];
                    h.free_look_pitch = c[7];
                }
            } else if player.control_mode == 1 {
                let c = &player.controls;
                // TODO: record 0x174..0x18c and 0x198, 0x19c take controls 9..15 and extra controls 2, 3, which no
                // client or bot ever sets (always 0); port them with their first reader
                h.gear_x_input = c[0];
                h.gear_y_input = c[2];
                h.strafe_input = c[1];
                h.walk_input = c[3];
                h.look_yaw = c[4];
                h.look_pitch = c[5];
                h.free_look_yaw = c[6];
                h.free_look_pitch = c[7];
                h.client_body_yaw = c[8];
                h.unk_190 = player.controls_extra[0];
                h.unk_194 = player.controls_extra[1];
                if held {
                    h.strafe_input = 0.0;
                    h.walk_input = 0.0;
                }
            } else {
                h.look_yaw = h.view_yaw;
                h.look_pitch = h.view_pitch;
            }
            h.input_flags = player.input_bits;
            h.movement_mode = player.zoom_level as i32;
        }
    }

    pub(crate) fn sync_humans(&mut self) {
        let map = &self.world.map;
        for (_, h) in self.humans.iter_mut() {
            sync_bones(h, &mut self.bodies, map);
        }
    }

    pub(crate) fn simulate_humans(&mut self) {
        let map = &mut self.world.map;
        let mut out = Vec::new();
        let mut touch = Touchables { grid: &self.item_grid, items: &mut self.items, types: &self.item_types, vehicles: &mut self.vehicles, vehicle_types: &self.vehicle_types, occupied: occupied_seats(&self.humans) };
        let mut deleted = Vec::new();
        let keep_bodies = matches!(self.gamemode, GameMode::Round | GameMode::Eliminator);
        for id in self.humans.ids() {
            let others = later_humans(&self.humans, id);
            let h = self.humans.get_mut(id).unwrap();
            if let HumanTick::Delete = simulate_human(id, h, &mut self.bodies, map, &mut touch, &others, &mut out, self.tick, &mut self.noise_seed, keep_bodies) {
                deleted.push(id);
            }
        }
        self.apply_human_outputs(out);
        self.apply_team_doors();
        for id in deleted {
            self.delete_human(id);
        }
    }

    pub(crate) fn apply_human_outputs(&mut self, out: Vec<HumanOutput>) {
        for o in out {
            let kind = match o {
                HumanOutput::Glass(b) => ServerEvent::BulletHole(EventBulletHole {
                    area: b.area,
                    block_x: b.block.x,
                    block_y: b.block.y,
                    block_z: b.block.z,
                    cell: b.cell,
                    face: b.face,
                    pos: Vector(b.pos),
                    vel: Vector(b.vel),
                }),
                HumanOutput::Blood(p) => ServerEvent::BulletHit(EventBulletHit { unk: 0, hit_type: 3, pos: Vector(p), normal: Vector(p) }),
                HumanOutput::ReleasePlayer(pid) => {
                    self.settle_death(pid);
                    continue;
                }
                HumanOutput::Sound { sound, pos, volume, pitch } => ServerEvent::Sound(EventSound { sound_type: sound, pos: Vector(pos), volume, pitch }),
                HumanOutput::RunOver { driver, victim } => {
                    self.score_run_over(driver, victim);
                    continue;
                }
                HumanOutput::DoorProbe { player, start, end, pos } => {
                    self.team_door_probe(player, start, end, pos);
                    continue;
                }
                HumanOutput::TaxAccount(account) => {
                    if let Some(a) = self.saved_accounts.get_player_data(account) {
                        economy::account_wealth_tax(a);
                    }
                    continue;
                }
            };
            self.events.push(Event { tick_created: self.tick, kind });
        }
    }

    pub fn simulate_human_at(&mut self, id: usize, ticks: u32, noise_seed: &mut i32) -> HumanTick {
        let map = &mut self.world.map;
        let mut out = Vec::new();
        let mut touch = Touchables { grid: &self.item_grid, items: &mut self.items, types: &self.item_types, vehicles: &mut self.vehicles, vehicle_types: &self.vehicle_types, occupied: occupied_seats(&self.humans) };
        let others = later_humans(&self.humans, id);
        let Some(h) = self.humans.get_mut(id) else { return HumanTick::Keep };
        let keep_bodies = matches!(self.gamemode, GameMode::Round | GameMode::Eliminator);
        let result = simulate_human(id, h, &mut self.bodies, map, &mut touch, &others, &mut out, ticks, noise_seed, keep_bodies);
        self.apply_human_outputs(out);
        result
    }

    pub fn calculate_arm_angles_at(&mut self, id: usize, noise_seed: &mut i32) {
        let map = &self.world.map;
        let touch = Touchables { grid: &self.item_grid, items: &mut self.items, types: &self.item_types, vehicles: &mut self.vehicles, vehicle_types: &self.vehicle_types, occupied: occupied_seats(&self.humans) };
        let Some(h) = self.humans.get_mut(id) else { return };
        crate::human::arms::calculate_arm_angles(h, &mut self.bodies, map, &touch, noise_seed);
    }

    pub(crate) fn delete_human(&mut self, id: usize) {
        let Some(mut h) = self.humans.remove(id) else { return };
        let mut touch = Touchables { grid: &self.item_grid, items: &mut self.items, types: &self.item_types, vehicles: &mut self.vehicles, vehicle_types: &self.vehicle_types, occupied: occupied_seats(&self.humans) };
        for slot in 0..h.inventory.len() {
            while h.inventory[slot].count > 0 {
                let item = h.inventory[slot].items[h.inventory[slot].count as usize - 1] as usize;
                let before = h.inventory[slot].count;
                detach_item(&mut h, &mut self.bodies, &mut touch, item, slot);
                if h.inventory[slot].count == before {
                    break;
                }
            }
        }
        // TODO: the binary bumps the player's update counter here rather than sending an event
        if let Some(player) = h.player.and_then(|p| self.players.get_mut(p.idx()))
            && player.human == Some(id)
        {
            player.human = None;
            self.events.push(player.make_update_player_event(self.tick));
        }
        for joint in &h.joints {
            self.bodies.remove_bond(joint.bond);
        }
        for bone in &h.bones {
            self.bodies.remove(bone.body);
        }
    }

    pub(crate) fn kill_human_for(&mut self, player_id: PlayerId) {
        let Some(player) = self.players.get(player_id.idx()) else { return };
        let eye = player.camera_pos.0;
        let target = self.humans.iter().filter(|(_, h)| h.old_health > 0).min_by(|(_, a), (_, b)| (a.pos - eye).length().total_cmp(&(b.pos - eye).length())).map(|(id, _)| id);
        let Some(id) = target else { return println!("[Sim] No living human to kill") };
        if let Some(h) = self.humans.get_mut(id) {
            h.old_health = 0;
        }
        println!("[Sim] Killed human #{id}");
    }

    pub fn human_mut(&mut self, id: usize) -> Option<&mut Human> {
        self.humans.get_mut(id)
    }

    pub fn human(&self, id: usize) -> Option<&Human> {
        self.humans.get(id)
    }

    /// A human and the level and vehicles its feet stand on (without the nearby vehicles it would trace).
    pub fn human_and_map(&mut self, id: usize) -> (&mut Human, crate::human::locomotion::Surface<'_>) {
        let surface = crate::human::locomotion::Surface { map: &self.world.map, vehicles: &self.vehicles, vehicle_types: &self.vehicle_types, nearby: &[] };
        (self.humans.get_mut(id).unwrap(), surface)
    }

    pub fn human_parts(&mut self, id: usize) -> (&mut Human, &mut RigidBodies, crate::human::locomotion::Surface<'_>) {
        let surface = crate::human::locomotion::Surface { map: &self.world.map, vehicles: &self.vehicles, vehicle_types: &self.vehicle_types, nearby: &[] };
        (self.humans.get_mut(id).unwrap(), &mut self.bodies, surface)
    }

    pub(crate) fn human_objects(&self) -> Vec<ServerHumanObject> {
        self.humans
            .iter()
            .map(|(id, h)| {
                let root = &h.bones[0];
                ServerHumanObject {
                    slot: id as u16,
                    human_id: id as u16,
                    player_id: h.player.map_or(-1, |p| p.0 as i32),
                    customization: h.customization,
                    vehicle: h.vehicle.map_or(-1, |v| v as i32),
                    vehicle_seat: h.seat as i32,
                    alive: h.old_health > 0,
                    bleeding: h.bleeding,
                    pos: Vector(root.pos),
                    rot: rot_matrix_to_quaternion(&root.rot),
                    bones: std::array::from_fn(|i| rot_matrix_to_quaternion(&h.bones[i + 1].networked_rot)),
                }
            })
            .collect()
    }

    /// The own-human block of each player's game packet (written by server_send while the player has a human).
    pub(crate) fn own_human_data(&self) -> std::collections::HashMap<usize, OwnHumanData> {
        self.players
            .iter()
            .filter_map(|(idx, p)| {
                let id = p.human?;
                let h = self.humans.get(id)?;
                Some((
                    idx,
                    OwnHumanData {
                        human_id: id as i32,
                        view_yaw: h.view_yaw,
                        view_pitch: h.view_pitch,
                        yaw_offset: h.yaw_offset,
                        pitch_offset: h.pitch_offset,
                        body_yaw: h.body_yaw,
                        is_standing: h.is_standing,
                        pain: h.pain,
                        unk_6e08: h.action_type,
                        unk_6e10: h.action_duration,
                        unk_6e14: h.action_hand,
                        unk_6e18: h.action_slot,
                        progress_bar: h.progress_bar,
                        health: [h.health, h.chest_hp, h.head_hp, h.left_arm_hp, h.right_arm_hp, h.left_leg_hp, h.right_leg_hp],
                        stamina: h.stamina,
                        max_stamina: h.max_stamina,
                        head_vel: Vector(h.bones[3].vel),
                        inventory: std::array::from_fn(|k| {
                            let slot = &h.inventory[k];
                            slot.items[..slot.count as usize]
                                .iter()
                                .filter_map(|&i| self.items.get(i as usize))
                                .map(|item| {
                                    let kind = item.item_type as i32;
                                    if k > 1 {
                                        return kind;
                                    }
                                    let ty = &self.item_types[kind as usize];
                                    let mut shown = kind;
                                    if let Some(&mag) = item.children.first() {
                                        if ty.is_gun {
                                            let empty = self.items.get(mag).is_some_and(|m| m.state.left() == 0);
                                            shown |= if empty { 0x300 } else { 0x100 };
                                        } else if item.item_type == ItemKind::BriefcaseOpen {
                                            shown |= 0x100;
                                        }
                                    }
                                    if ty.magazine_ammo > 0 { shown | (item.state.left() << 8) } else { shown }
                                })
                                .collect()
                        }),
                    },
                ))
            })
            .collect()
    }

    pub fn bodies(&self) -> &RigidBodies {
        &self.bodies
    }

    pub fn bodies_mut(&mut self) -> &mut RigidBodies {
        &mut self.bodies
    }
}

/// The unseated humans after `id`, which human `id` collides with this tick.
fn later_humans(humans: &rosa_physics::Table<Human>, id: usize) -> Vec<crate::human::physics::OtherHuman> {
    humans.iter().filter(|&(k, h)| k > id && h.vehicle.is_none()).map(|(_, h)| crate::human::physics::OtherHuman::of(h)).collect()
}

/// The (vehicle, seat) pairs humans sit in.
pub(crate) fn occupied_seats(humans: &rosa_physics::Table<Human>) -> Vec<(usize, usize)> {
    humans.iter().filter_map(|(_, h)| h.vehicle.map(|v| (v, h.seat))).collect()
}
