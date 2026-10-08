use glam::Vec3;
use rosa_math::vector::Vector;
use rosa_physics::{
    RigidBodies, RotMatrix,
    rotation::{IDENTITY, rot_matrix_to_quaternion, rotate_orientation},
};
use rosa_protocol::clientbound::game::{OwnHumanData, ServerHumanObject, events::{Event, ServerEvent, bullet_hole::EventBulletHole}};

use super::{Sim, items::Touchables};
use crate::{
    PlayerId,
    human::{
        Human,
        create::create_human,
        physics::sync_bones,
        simulation::{HumanTick, simulate_human},
    },
};

const SPAWN_DISTANCE: f32 = 2.0;
pub const HUMAN_SLOTS: usize = crate::human::MAX_HUMANS;
pub const OBJECT_SLOTS: usize = 1024;

impl Sim {
    pub fn spawn_human(&mut self, pos: Vec3, rot: &RotMatrix, player: Option<PlayerId>) -> Option<usize> {
        let player = player.and_then(|p| self.players.get(p.idx()));
        create_human(&mut self.humans, &mut self.bodies, pos, rot, player)
    }

    pub(crate) fn spawn_human_for(&mut self, player_id: PlayerId) {
        let Some(player) = self.players.get(player_id.idx()) else { return };
        let mut rot = IDENTITY;
        rotate_orientation(&mut rot, Vec3::Y, player.view_yaw);
        let pos = player.camera_pos.0 - rot[0] * SPAWN_DISTANCE;
        let Some(id) = self.spawn_human(pos, &rot, Some(player_id)) else {
            return println!("[Sim] Human table full");
        };
        if let Some(player) = self.players.get_mut(player_id.idx()) {
            player.human = Some(id);
            self.events.push(player.make_update_player_event(self.tick));
        }
        println!("[Sim] Spawned human #{id} at {pos:?}");
    }

    /// player_simulation: hands each player's latest controls to their human.
    pub(crate) fn player_simulation(&mut self) {
        for (_, player) in self.players.iter_mut() {
            // TODO: spawn timer, stocks and account bookkeeping
            let Some(h) = player.human.and_then(|id| self.humans.get_mut(id)) else { continue };
            if h.vehicle.is_some() {
                // TODO: vehicle controls (control modes 2 and 3 copy the first 8 control floats)
            } else if player.control_mode == 1 {
                let c = &player.controls;
                // TODO: record 0x150, 0x158, 0x174, 0x178 and 0x17c..0x18c (controls 0, 2, 9, 10 and 11..15) and
                // 0x198, 0x19c (extra controls 2 and 3) are not ported
                h.strafe_input = c[1];
                h.walk_input = c[3];
                h.unk_160 = c[4];
                h.unk_164 = c[5];
                h.unk_168 = c[6];
                h.unk_16c = c[7];
                h.unk_170 = c[8];
                h.unk_190 = player.controls_extra[0];
                h.unk_194 = player.controls_extra[1];
                // TODO: in versus mode the human cannot move during the start delay
            } else {
                h.unk_160 = h.view_yaw;
                h.unk_164 = h.view_pitch;
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
        let mut breaks = Vec::new();
        let mut touch = Touchables { grid: &self.item_grid, items: &mut self.items, types: &self.item_types };
        let mut deleted = Vec::new();
        for id in self.humans.ids() {
            let others = later_humans(&self.humans, id);
            let h = self.humans.get_mut(id).unwrap();
            if let HumanTick::Delete = simulate_human(id, h, &mut self.bodies, map, &mut touch, &others, &mut breaks, self.tick, &mut self.noise_seed) {
                deleted.push(id);
            }
        }
        self.announce_glass_breaks(breaks);
        for id in deleted {
            self.delete_human(id);
        }
    }

    fn announce_glass_breaks(&mut self, breaks: Vec<crate::human::physics::GlassBreak>) {
        for b in breaks {
            self.events.push(Event {
                tick_created: self.tick,
                kind: ServerEvent::BulletHole(EventBulletHole {
                    area: b.area,
                    block_x: b.block.x,
                    block_y: b.block.y,
                    block_z: b.block.z,
                    cell: b.cell,
                    face: b.face,
                    pos: Vector(b.pos),
                    vel: Vector(b.vel),
                }),
            });
        }
    }

    pub fn simulate_human_at(&mut self, id: usize, ticks: u32, noise_seed: &mut i32) -> HumanTick {
        let map = &mut self.world.map;
        let mut breaks = Vec::new();
        let mut touch = Touchables { grid: &self.item_grid, items: &mut self.items, types: &self.item_types };
        let others = later_humans(&self.humans, id);
        let Some(h) = self.humans.get_mut(id) else { return HumanTick::Keep };
        simulate_human(id, h, &mut self.bodies, map, &mut touch, &others, &mut breaks, ticks, noise_seed)
    }

    pub fn calculate_arm_angles_at(&mut self, id: usize, noise_seed: &mut i32) {
        let map = &self.world.map;
        let touch = Touchables { grid: &self.item_grid, items: &mut self.items, types: &self.item_types };
        let Some(h) = self.humans.get_mut(id) else { return };
        crate::human::arms::calculate_arm_angles(h, &mut self.bodies, map, &touch, noise_seed);
    }

    pub(crate) fn delete_human(&mut self, id: usize) {
        // TODO: the binary's delete_human also tells the clients
        let Some(h) = self.humans.remove(id) else { return };
        for slot in &h.inventory {
            for &item in &slot.items[..slot.count as usize] {
                if let Some(item) = self.items.get_mut(item as usize) {
                    item.parent_human = -1;
                }
            }
        }
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

    pub fn human_and_map(&mut self, id: usize) -> (&mut Human, &crate::world::map::Map) {
        (self.humans.get_mut(id).unwrap(), &self.world.map)
    }

    pub fn human_parts(&mut self, id: usize) -> (&mut Human, &mut RigidBodies, &crate::world::map::Map) {
        (self.humans.get_mut(id).unwrap(), &mut self.bodies, &self.world.map)
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
                    vehicle_seat: 0,
                    alive: h.old_health > 0,
                    bleeding: false,
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
                        // TODO: human record 0x124 is not ported yet
                        unk_124: 0.0,
                        unk_100: h.unk_100,
                        is_standing: h.is_standing,
                        pain: h.pain,
                        unk_6e08: h.action_type,
                        unk_6e10: h.action_duration,
                        unk_6e14: h.action_hand,
                        unk_6e18: h.action_slot,
                        // TODO: the progress bar (record 0x6e04)
                        progress_bar: 0,
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
                                    let ty = &self.item_types[kind as usize];
                                    // TODO: guns show a loaded (0x100) or empty (0x300) magazine and the briefcase
                                    // (0x10) whether it holds anything; items do not track bullets yet, so magazines
                                    // show the full count
                                    if ty.magazine_ammo > 0 { kind | (ty.magazine_ammo << 8) } else { kind }
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
