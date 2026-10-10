use glam::Vec3;
use rosa_physics::{
    RigidBodies,
    rotation::{IDENTITY, rotate_orientation},
};

use super::{
    Human,
    arms::{calculate_arm_angles, calculate_spread_vector},
    inventory::{action_simulation, hand_grab_and_inventory, unlink_item},
    bones::BoneId,
    locomotion::{FOOT_FREE, FOOT_PLANTED, Surface, calculate_center_of_mass, slide_simulation, start_step, step_locomotion_ik, update_foot_ground_constraint, update_locomotion_constraints},
    physics::{HumanOutput, OtherHuman, bone_item_contacts, bone_track_contacts, bone_world_contacts, find_nearby_vehicles, human_contacts, joint_limits, update_networked_bones, vehicle_contacts},
    seated::{ENTER_KEY, fall_out, simulate_seated, walk_simulation},
};
use crate::{sim::items::Touchables, world::map::Map};

const TURN_STEP: f32 = 3.0 / 64.0;
const DESPAWN_TICKS: i32 = 3600;
const DEAD_PLAYER_TICKS: i32 = 3480;

/// What human_simulation asks of the sim after a human's tick.
pub enum HumanTick {
    Keep,
    Delete,
}

/// human_simulation for one human: health, turning towards the view, balance and walking for the conscious, the
/// ragdoll for the dead, and the contacts holding the bones against the world. Round and eliminator mode keep dead
/// bodies (`keep_bodies`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn simulate_human(id: usize, h: &mut Human, bodies: &mut RigidBodies, map: &mut Map, touch: &mut Touchables, others: &[OtherHuman], out: &mut Vec<HumanOutput>, ticks: u32, noise_seed: &mut i32, keep_bodies: bool) -> HumanTick {
    find_nearby_vehicles(h, touch);
    let nearby = h.nearby_vehicles.clone();
    let seated = h.vehicle;
    health_sim(id, h, ticks, out);
    if ticks & 0x1f == 0 {
        if h.stamina > h.max_stamina {
            h.stamina = h.max_stamina;
        }
        let cap = if h.max_stamina > 127 { 127 } else { h.max_stamina };
        if cap > h.stamina {
            h.stamina += 1;
        }
    }
    let v = h.unk_6adc;
    let len = (v.z * v.z + (v.y * v.y + v.x * v.x)).sqrt();
    h.unk_6adc = if len > (1.0 / 1024.0) {
        let k = 63.0 / 64.0;
        Vec3::new(v.x * k, v.y * k, k * v.z)
    } else {
        Vec3::ZERO
    };
    if h.spawn_protection > 0 {
        h.spawn_protection -= 1;
    }
    // TODO: the stance handling (with the crouch input) when the server setting at 0xa8ce184 is off
    let base_strength = 1.0f32;
    if ticks & 7 == 0 && h.pain > 0 {
        h.pain -= 1;
    }
    if h.eat_cooldown > 0 {
        h.eat_cooldown -= 1;
    }
    if h.vehicle.is_some() {
        h.yaw_offset = h.look_yaw;
        h.pitch_offset = h.look_pitch;
        h.unk_128 = h.look_yaw;
        h.unk_12c = h.look_pitch;
    } else {
        turn_towards_view(h);
    }
    // TODO: the player's network state flag at 0x6fd4 (from the owning player's state)

    h.strength = base_strength;
    if h.old_health <= 24 {
        h.strength = weak_strength(id, h.old_health, ticks, base_strength);
    }
    let has_item = h.inventory[0].count > 0 || h.inventory[1].count > 0;

    h.is_on_ground = false;
    for (j, bone) in h.bones.iter_mut().enumerate() {
        bone.strength = if seated.is_some() && (j == 0 || j > 9) { 0.0 } else { base_strength };
        bone.ground_contact = 0;
        if (j == BoneId::HandLeft as usize || j == BoneId::HandRight as usize)
            && let Some(b) = bodies.get_mut(bone.body)
        {
            b.mass = 1.5;
        }
    }
    for bone in &h.bones {
        if let Some(b) = bodies.get_mut(bone.body) {
            b.settled = bone.strength == 0.0;
        }
    }
    calculate_center_of_mass(h);

    let mut heading = IDENTITY;
    let axis = heading[1];
    rotate_orientation(&mut heading, axis, h.view_yaw);
    let r2 = heading[2];
    for (k, foot) in [BoneId::FootLeft as usize, BoneId::FootRight as usize].into_iter().enumerate() {
        let f = &mut h.locomotion.feet[k];
        f.prev_plant_pitch = f.plant_pitch;
        f.plant_pitch = 0.0;
        let (p, q) = (h.bones[0].pos, h.bones[foot].pos);
        let s = ((r2.x * (p.x - q.x) + r2.y * (p.y - q.y)) + (p.z - q.z) * r2.z) + 0.1875;
        let mut pitch = 0.0;
        if 0.0 > s {
            pitch = s * 3.0;
            if -90.0_f64.to_radians() > pitch as f64 {
                pitch = -90.0_f32.to_radians();
            }
        }
        h.locomotion.feet[k].plant_pitch = pitch;
    }

    if let Some(vid) = seated {
        simulate_seated(h, bodies, map, touch, vid, has_item, base_strength);
        if h.old_health <= 0 || h.health <= 49 {
            fall_out(h, touch, vid);
        }
        if h.input_flags & ENTER_KEY != 0 && h.last_input_flags & ENTER_KEY == 0 {
            h.seat_exit = 1;
        }
    } else {
        walk_and_collide(h, bodies, map, touch, &nearby, out);
    }
    if h.old_health > 0 {
        hand_grab_and_inventory(h, id, bodies, touch, out);
        action_simulation(h, id, bodies, touch, out);
    } else {
        drop_everything(h, bodies, touch, noise_seed);
    }
    for bone in h.bones.iter_mut() {
        let Some(b) = bodies.get(bone.body) else { continue };
        bone.pos = b.pos;
        bone.vel = b.vel;
        bone.rot = b.rot;
        bone.ang_vel = b.ang_vel;
    }
    calculate_arm_angles(h, bodies, map, touch, noise_seed);
    simulate_movement(h, bodies, &Surface { map, vehicles: touch.vehicles, vehicle_types: touch.vehicle_types, nearby: &nearby }, ticks);
    if h.old_health <= 0 || h.health <= 49 {
        joint_limits(h, bodies);
    }

    h.pos = h.bones[0].pos;
    if -32.0 > h.bones[0].pos.y {
        h.old_health = -100;
    }
    if h.vehicle.is_none() {
        bone_track_contacts(h, bodies, &map.level.area.track);
        bone_item_contacts(h, bodies, touch);
        human_contacts(h, bodies, others);
    }

    let result = despawn_timer(h, out, keep_bodies);
    h.last_input_flags = h.input_flags;
    update_networked_bones(h);
    result
}

/// The part of human_simulation for a human on foot: the jump charging, the feet held to the ground, the bones
/// against the level and against nearby vehicles.
fn walk_and_collide(h: &mut Human, bodies: &mut RigidBodies, map: &mut Map, touch: &mut Touchables, nearby: &[usize], out: &mut Vec<HumanOutput>) {
    let alive = h.old_health > 0;
    let standing = alive && h.health > 49;
    if alive && h.movement_state == 0 && h.input_flags & 4 != 0 && h.locomotion.jump_charge <= 31 {
        h.locomotion.jump_charge += 1;
    }
    for bone in h.bones.iter_mut() {
        bone.ground_contact = 0;
    }
    if standing && h.movement_state != 2 {
        let surface = Surface { map, vehicles: touch.vehicles, vehicle_types: touch.vehicle_types, nearby };
        update_foot_ground_constraint(h, bodies, &surface, 0);
        update_foot_ground_constraint(h, bodies, &surface, 1);
    }
    let feet_down = h.locomotion.feet[0].mode == FOOT_PLANTED
        && h.locomotion.feet[1].mode == FOOT_PLANTED
        && h.bones[BoneId::FootLeft as usize].ground_contact != 0
        && h.bones[BoneId::FootRight as usize].ground_contact != 0;
    if h.movement_state != 3 && h.locomotion.jump_charge > 0 && h.input_flags & 4 == 0 && feet_down {
        start_step(h, 1);
    }
    bone_world_contacts(h, bodies, map, out);
    if h.last_vehicle_cooldown > 0 {
        h.last_vehicle_cooldown -= 1;
    } else {
        h.last_vehicle = -1;
    }
    vehicle_contacts(h, bodies, touch, out);
    h.pos = h.bones[0].pos;
}

/// A dead human lets go of the first item in every slot, which flies off with the pelvis's speed plus a little
/// random spin.
fn drop_everything(h: &mut Human, bodies: &mut RigidBodies, touch: &mut Touchables, noise_seed: &mut i32) {
    for slot in 0..h.inventory.len() {
        if h.inventory[slot].count <= 0 {
            continue;
        }
        let item_id = h.inventory[slot].items[0] as usize;
        let Some(body) = touch.items.get(item_id).map(|item| item.body) else { continue };
        let vel = h.bones[0].vel;
        let s = calculate_spread_vector(noise_seed, 1.0 / 60.0, 0.0);
        if let Some(b) = bodies.get_mut(body) {
            b.vel = Vec3::new(vel.x + s.x, vel.y + s.y, vel.z + s.z);
        }
        if let Some(item) = touch.items.get_mut(item_id) {
            item.physics_settled = false;
            item.settled_timer = 0;
        }
        unlink_item(h, bodies, touch, item_id);
    }
}

/// A dead body's timer is held at 4 in round and eliminator mode, so it never despawns.
const KEPT_BODY_TICKS: i32 = 4;

fn despawn_timer(h: &mut Human, out: &mut Vec<HumanOutput>, keep_bodies: bool) -> HumanTick {
    let alive = h.old_health > 0;
    if h.old_health > 0 {
        if h.player.is_some() {
            h.despawn_ticks = DESPAWN_TICKS;
            return HumanTick::Keep;
        }
    } else if h.despawn_ticks < DEAD_PLAYER_TICKS
        && let Some(player) = h.player
    {
        out.push(HumanOutput::ReleasePlayer(player));
        h.player = None;
        h.account = None;
    } else if h.despawn_ticks < DEAD_PLAYER_TICKS
        && let Some(account) = h.account.take()
    {
        out.push(HumanOutput::TaxAccount(account));
    }
    if keep_bodies && !alive && h.despawn_ticks - 1 < KEPT_BODY_TICKS {
        h.despawn_ticks = KEPT_BODY_TICKS;
        return HumanTick::Keep;
    }
    h.despawn_ticks -= 1;
    if h.despawn_ticks > 0 { HumanTick::Keep } else { HumanTick::Delete }
}

fn turn_towards_view(h: &mut Human) {
    let (a, b) = (h.client_body_yaw, h.body_yaw);
    let mut b2 = b;
    if ((a - b) as f64) >= 180.0_f64.to_radians() {
        b2 = (b as f64 + 360.0_f64.to_radians()) as f32;
    }
    let mut d = b2 - a;
    if (d as f64) >= 180.0_f64.to_radians() {
        d = b2 - (a as f64 + 360.0_f64.to_radians()) as f32;
    }
    let mut c = h.look_yaw;
    if ((d - c) as f64) >= 180.0_f64.to_radians() {
        c = (c as f64 + 360.0_f64.to_radians()) as f32;
    }
    let mut e = c - d;
    if (e as f64) >= 180.0_f64.to_radians() {
        e = c - (d as f64 + 360.0_f64.to_radians()) as f32;
    }
    h.view_turn = e;
    let step = e.clamp(-TURN_STEP, TURN_STEP);
    let (step, rest) = if h.is_standing { (step, e - step) } else { (0.0, e) };
    h.yaw_offset = if -90.0_f64.to_radians() > rest as f64 {
        -90.0_f32.to_radians()
    } else if rest as f64 > 90.0_f64.to_radians() {
        90.0_f32.to_radians()
    } else {
        rest
    };
    let mut body = b + step;
    let mut yaw = h.view_yaw + step;
    yaw += h.locomotion.support_ang_vel.y;
    let tiny = 1.0 / 65536.0;
    if tiny > yaw.abs() {
        yaw = 0.0;
        if tiny > body.abs() {
            body = 0.0;
        }
    } else {
        if tiny > body.abs() {
            body = 0.0;
        }
        if -180.0_f64.to_radians() > yaw as f64 {
            yaw = (yaw as f64 + 360.0_f64.to_radians()) as f32;
        } else if yaw as f64 > 180.0_f64.to_radians() {
            yaw = (yaw as f64 - 360.0_f64.to_radians()) as f32;
        }
    }
    if -180.0_f64.to_radians() > body as f64 {
        body = (body as f64 + 360.0_f64.to_radians()) as f32;
    } else if body as f64 > 180.0_f64.to_radians() {
        body = (body as f64 - 360.0_f64.to_radians()) as f32;
    }
    h.view_yaw = yaw;
    h.body_yaw = body;
    h.view_pitch = h.look_pitch;
    h.unk_128 = 0.0;
    h.unk_12c = 0.0;
}

/// How much of its strength a badly hurt human has left, wavering over time.
fn weak_strength(id: usize, old_health: i32, ticks: u32, base: f32) -> f32 {
    let id = id as i32;
    let t = (id.wrapping_mul(id).wrapping_mul(id).wrapping_add(ticks as i32)) & 0x1ff;
    let x = t as f32 * (1.0 / 512.0);
    let s = ((x as f64 * 360.0_f64.to_radians()) as f32 as f64).sin();
    let hp = old_health as f32;
    let a = hp / 25.0;
    let mut r = (s * 0.25 * (base - a) as f64 + (a * 0.5 + 0.375) as f64) as f32;
    if old_health <= 9 {
        r *= (hp / 10.0) * 0.5 + 0.5;
    }
    r
}

/// human_health_sim: bleeding, regeneration and death from wounds.
///
/// Health, blood and the body parts each heal by one point (up to 100) on their own schedule:
/// - immortal: health every 8 ticks, parts every 16, blood every 256;
/// - mortal and bleeding: health every 64 ticks, nothing else;
/// - mortal: health every 32 ticks, parts every 64, blood every 256.
fn health_sim(id: usize, h: &mut Human, ticks: u32, out: &mut Vec<HumanOutput>) {
    if h.is_immortal {
        if h.down_timer > 0 {
            h.down_timer -= 1;
        }
        if h.health <= 49 {
            h.down_timer = 1800;
        }
    } else if h.health <= 74 {
        h.pain = h.pain.max(((75 - h.health) * 2).min(60));
    }

    if h.old_health <= 0 {
        h.bleeding = false;
        h.health = 0;
        return;
    }

    h.health = h.health.max(0);
    for hp in parts(h) {
        *hp = (*hp).max(0);
    }

    let every = |n: u32| ticks.is_multiple_of(n);
    if h.is_immortal {
        if every(8) {
            heal(&mut h.health);
        }
        if every(16) {
            parts(h).into_iter().for_each(heal);
        }
        if every(256) {
            heal(&mut h.blood_level);
        }
    } else {
        if h.chest_hp <= 0 || h.head_hp <= 0 {
            h.old_health = 0;
        }
        if h.bleeding {
            if every(64) {
                heal(&mut h.health);
            }
        } else {
            if every(32) {
                heal(&mut h.health);
            }
            if every(64) {
                parts(h).into_iter().for_each(heal);
            }
            if every(256) {
                heal(&mut h.blood_level);
            }
        }
    }

    if h.bleeding {
        bleed(id, h, ticks, out);
    }

    if !h.is_immortal && h.blood_level <= 10 {
        h.old_health = 0;
    }
}

/// One point back, up to 100.
fn heal(v: &mut i32) {
    if *v <= 99 {
        *v += 1;
    }
}

fn parts(h: &mut Human) -> [&mut i32; 6] {
    [&mut h.chest_hp, &mut h.head_hp, &mut h.left_arm_hp, &mut h.right_arm_hp, &mut h.left_leg_hp, &mut h.right_leg_hp]
}

fn bleed(id: usize, h: &mut Human, ticks: u32, out: &mut Vec<HumanOutput>) {
    let blood = h.blood_level;
    if blood <= 19 {
        let cap = blood * 2 + 10;
        if h.health > cap {
            h.health = cap;
        }
        if blood <= 0 {
            return;
        }
    }
    let id = id as i32;
    if (id.wrapping_mul(id).wrapping_mul(id) ^ ticks as i32) as u8 == 0 && !h.is_immortal {
        h.blood_level = blood - 3;
        let p = h.bones[1].pos;
        out.push(HumanOutput::Blood(Vec3::new(0.0 * 0.5 + p.x, 1.0 * 0.5 + p.y, 0.5 * 0.0 + p.z)));
    }
}

/// human_simulate_movement: picks the movement simulation for the human's movement state.
fn simulate_movement(h: &mut Human, bodies: &mut RigidBodies, surface: &Surface, ticks: u32) {
    if 0.707 > h.bones[0].rot[1].y {
        h.locomotion.jump_charge = 0;
    }
    if h.vehicle.is_some() {
        h.movement_state = 4;
        h.locomotion.feet[0].mode = FOOT_FREE;
        h.locomotion.feet[1].mode = FOOT_FREE;
    } else {
        match h.movement_state {
            4 => {
                h.movement_state = 1;
                h.locomotion.active_foot = 0;
            }
            2 if h.input_flags & 0x80000 == 0 => h.movement_state = 0,
            3 => return step_locomotion_ik(h, bodies, surface),
            _ => {}
        }
    }
    if h.input_flags & 0x80000 != 0 {
        h.movement_state = 2;
    }
    match h.movement_state {
        0 | 1 | 5 => update_locomotion_constraints(h, bodies, surface, ticks),
        2 => slide_simulation(h, bodies),
        3 => step_locomotion_ik(h, bodies, surface),
        4 => walk_simulation(h, bodies),
        _ => {}
    }
}
