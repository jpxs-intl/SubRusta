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
    locomotion::{FOOT_FREE, FOOT_PLANTED, calculate_center_of_mass, slide_simulation, start_step, step_locomotion_ik, update_foot_ground_constraint, update_locomotion_constraints},
    physics::{HumanOutput, OtherHuman, bone_item_contacts, bone_world_contacts, human_contacts, joint_limits, update_networked_bones},
};
use crate::{sim::items::Touchables, world::map::Map};

const PI: f64 = 3.14159265359;
const TWO_PI: f64 = 6.28318530718;
const HALF_PI: f64 = 1.570796326795;
const TURN_STEP: f32 = 0.046875;
const DESPAWN_TICKS: i32 = 3600;
const DEAD_PLAYER_TICKS: i32 = 3480;

/// What human_simulation asks of the sim after a human's tick.
pub enum HumanTick {
    Keep,
    Delete,
}

/// human_simulation for one human: health, turning towards the view, balance and walking for the conscious, the
/// ragdoll for the dead, and the contacts holding the bones against the world.
#[allow(clippy::too_many_arguments)]
pub(crate) fn simulate_human(id: usize, h: &mut Human, bodies: &mut RigidBodies, map: &mut Map, touch: &mut Touchables, others: &[OtherHuman], out: &mut Vec<HumanOutput>, ticks: u32, noise_seed: &mut i32) -> HumanTick {
    h.unk_218 = 0;
    // TODO: the scan over vehicles for one this human is grabbing
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
    h.unk_6adc = if len > 0.0009765625 {
        let k = 0.984375;
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
    if h.unk_40 > 0 {
        h.unk_40 -= 1;
    }
    // TODO: turning while seated in a vehicle
    if h.vehicle.is_none() {
        turn_towards_view(h);
    }
    // TODO: the player's network state flag at 0x6fd4 (from the owning player's state)

    h.strength = base_strength;
    if h.old_health <= 24 {
        h.strength = weak_strength(id, h.old_health, ticks, base_strength);
    }
    // TODO: whether the human holds an item (inventory slots) is read here for the walking code

    h.is_on_ground = false;
    for (j, bone) in h.bones.iter_mut().enumerate() {
        // TODO: seated bones are handled here when in a vehicle; the per-bone strength (bone +0xc8) is set to `base_strength`
        bone.ground_contact = 0;
        if (j == BoneId::HandLeft as usize || j == BoneId::HandRight as usize)
            && let Some(b) = bodies.get_mut(bone.body)
        {
            b.mass = 1.5;
        }
    }
    for bone in &h.bones {
        if let Some(b) = bodies.get_mut(bone.body) {
            b.settled = false;
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
            if -HALF_PI > pitch as f64 {
                pitch = -1.5707964;
            }
        }
        h.locomotion.feet[k].plant_pitch = pitch;
    }

    let alive = h.old_health > 0;
    let standing = alive && h.health > 49;
    // TODO: vehicles
    if alive && h.movement_state == 0 && h.input_flags & 4 != 0 && h.locomotion.jump_charge <= 31 {
        h.locomotion.jump_charge += 1;
    }
    for bone in h.bones.iter_mut() {
        bone.ground_contact = 0;
    }
    if standing && h.movement_state != 2 {
        update_foot_ground_constraint(h, bodies, map, 0);
        update_foot_ground_constraint(h, bodies, map, 1);
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
    // TODO: the 0x218 counter (set while grabbed) is handled here
    h.pos = h.bones[0].pos;
    if h.old_health > 0 {
        hand_grab_and_inventory(h, id, bodies, touch);
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
    simulate_movement(h, bodies, map, ticks);
    if h.old_health <= 0 || h.health <= 49 {
        joint_limits(h, bodies);
    }

    h.pos = h.bones[0].pos;
    h.unk_2c = 1;
    // TODO: seated humans skip the world contacts below
    if -32.0 > h.bones[0].pos.y {
        h.old_health = -100;
    }
    // TODO: train track triangles come first in human_generate_world_item_self_contacts
    bone_item_contacts(h, bodies, touch);
    human_contacts(h, bodies, others);

    let result = despawn_timer(h, out);
    h.last_input_flags = h.input_flags;
    update_networked_bones(h);
    result
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
        let s = calculate_spread_vector(noise_seed, 0.016666668, 0.0);
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

fn despawn_timer(h: &mut Human, out: &mut Vec<HumanOutput>) -> HumanTick {
    if h.old_health > 0 {
        if h.player.is_some() {
            h.despawn_ticks = DESPAWN_TICKS;
            return HumanTick::Keep;
        }
    } else if h.despawn_ticks < DEAD_PLAYER_TICKS
        && let Some(player) = h.player
    {
        // TODO: the game mode bookkeeping on death (world mode stocks, eliminator and versus scores, saved
        // inventories, account cash) and versus's other limit
        out.push(HumanOutput::ReleasePlayer(player));
        h.player = None;
        h.account = None;
    } else if h.despawn_ticks < DEAD_PLAYER_TICKS
        && let Some(account) = h.account.take()
    {
        out.push(HumanOutput::TaxAccount(account));
    }
    // TODO: in game modes 3 and 5 a dead human's timer stops at 3
    h.despawn_ticks -= 1;
    if h.despawn_ticks > 0 { HumanTick::Keep } else { HumanTick::Delete }
}

fn turn_towards_view(h: &mut Human) {
    let (a, b) = (h.unk_170, h.unk_100);
    let mut b2 = b;
    if ((a - b) as f64) >= PI {
        b2 = (b as f64 + TWO_PI) as f32;
    }
    let mut d = b2 - a;
    if (d as f64) >= PI {
        d = b2 - (a as f64 + TWO_PI) as f32;
    }
    let mut c = h.unk_160;
    if ((d - c) as f64) >= PI {
        c = (c as f64 + TWO_PI) as f32;
    }
    let mut e = c - d;
    if (e as f64) >= PI {
        e = c - (d as f64 + TWO_PI) as f32;
    }
    h.unk_e0 = e;
    let step = e.clamp(-TURN_STEP, TURN_STEP);
    let (step, rest) = if h.is_standing { (step, e - step) } else { (0.0, e) };
    h.yaw_offset = if -HALF_PI > rest as f64 {
        -1.5707964
    } else if rest as f64 > HALF_PI {
        1.5707964
    } else {
        rest
    };
    let mut body = b + step;
    let mut yaw = h.view_yaw + step;
    yaw += h.locomotion.support_ang_vel.y;
    let tiny = 1.5258789e-5;
    if tiny > yaw.abs() {
        yaw = 0.0;
        if tiny > body.abs() {
            body = 0.0;
        }
    } else {
        if tiny > body.abs() {
            body = 0.0;
        }
        if -PI > yaw as f64 {
            yaw = (yaw as f64 + TWO_PI) as f32;
        } else if yaw as f64 > PI {
            yaw = (yaw as f64 - TWO_PI) as f32;
        }
    }
    if -PI > body as f64 {
        body = (body as f64 + TWO_PI) as f32;
    } else if body as f64 > PI {
        body = (body as f64 - TWO_PI) as f32;
    }
    h.view_yaw = yaw;
    h.unk_100 = body;
    h.view_pitch = h.unk_164;
    h.unk_128 = 0.0;
    h.unk_12c = 0.0;
}

/// How much of its strength a badly hurt human has left, wavering over time.
fn weak_strength(id: usize, old_health: i32, ticks: u32, base: f32) -> f32 {
    let id = id as i32;
    let t = (id.wrapping_mul(id).wrapping_mul(id).wrapping_add(ticks as i32)) & 0x1ff;
    let x = t as f32 * 0.001953125;
    let s = ((x as f64 * TWO_PI) as f32 as f64).sin();
    let hp = old_health as f32;
    let a = hp / 25.0;
    let mut r = (s * 0.25 * (base - a) as f64 + (a * 0.5 + 0.375) as f64) as f32;
    if old_health <= 9 {
        r *= (hp / 10.0) * 0.5 + 0.5;
    }
    r
}

/// human_health_sim: bleeding, regeneration and death from wounds.
fn health_sim(id: usize, h: &mut Human, ticks: u32, out: &mut Vec<HumanOutput>) {
    // TODO: game mode 6 and the owning player's flag (player +0x2d18) kill humans below 50 health here
    if h.unk_68 != 0 {
        if h.unk_6c > 0 {
            h.unk_6c -= 1;
        }
        if h.health <= 49 {
            h.unk_6c = 1800;
        }
    } else if h.health <= 74 {
        let p = (75 - h.health) * 2;
        let p = if p > 60 { 60 } else { p };
        if h.pain < p {
            h.pain = p;
        }
    }
    if h.old_health <= 0 {
        h.unk_6d80 = 0;
        h.health = 0;
        return;
    }
    if h.health < 0 {
        h.health = 0;
    }
    for hp in [&mut h.chest_hp, &mut h.head_hp, &mut h.left_arm_hp, &mut h.right_arm_hp, &mut h.left_leg_hp, &mut h.right_leg_hp] {
        if *hp < 0 {
            *hp = 0;
        }
    }
    let bleeding = h.unk_6d80 != 0;
    let unconscious = h.unk_68 != 0;
    let (mut stage, mut fast) = (0, false);
    if unconscious {
        if ticks & 7 == 0 {
            if h.health <= 99 {
                h.health += 1;
            }
            fast = true;
        } else if ticks as u8 == 0 && h.blood_level <= 99 {
            h.blood_level += 1;
            (stage, fast) = (1, true);
        } else {
            if ticks & 0xf == 0 {
                regen_parts(h);
            }
            stage = 2;
        }
    } else {
        if h.chest_hp <= 0 {
            h.old_health = 0;
        }
        if h.head_hp <= 0 {
            h.old_health = 0;
        }
        let due = if bleeding { ticks & 0x3f == 0 } else { ticks & 0x1f == 0 };
        if due && h.health <= 99 {
            h.health += 1;
        }
        if bleeding {
            stage = 3;
        }
    }
    if stage == 0 {
        if ticks as u8 == 0 && h.blood_level <= 99 {
            h.blood_level += 1;
        }
        stage = 1;
    }
    if stage == 1 {
        let mask = if fast { 0xf } else { 0x3f };
        if ticks & mask == 0 {
            regen_parts(h);
        }
        stage = 2;
    }
    if stage == 2 && bleeding {
        stage = 3;
    }
    if stage == 3 {
        bleed(id, h, ticks, out);
    }
    if h.unk_68 == 0 && h.blood_level <= 10 {
        h.old_health = 0;
    }
}

fn regen_parts(h: &mut Human) {
    for hp in [&mut h.chest_hp, &mut h.head_hp, &mut h.left_arm_hp, &mut h.right_arm_hp, &mut h.left_leg_hp, &mut h.right_leg_hp] {
        if *hp <= 99 {
            *hp += 1;
        }
    }
}

fn bleed(id: usize, h: &mut Human, ticks: u32, out: &mut Vec<HumanOutput>) {
    // TODO: game mode 6 stops bleeding at random (1 in 512 ticks)
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
    if (id.wrapping_mul(id).wrapping_mul(id) ^ ticks as i32) as u8 == 0 && h.unk_68 == 0 {
        h.blood_level = blood - 3;
        let p = h.bones[1].pos;
        out.push(HumanOutput::Blood(Vec3::new(0.0 * 0.5 + p.x, 1.0 * 0.5 + p.y, 0.5 * 0.0 + p.z)));
    }
}

/// human_simulate_movement: picks the movement simulation for the human's movement state.
fn simulate_movement(h: &mut Human, bodies: &mut RigidBodies, map: &Map, ticks: u32) {
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
            3 => return step_locomotion_ik(h, bodies, map),
            _ => {}
        }
    }
    if h.input_flags & 0x80000 != 0 {
        h.movement_state = 2;
    }
    match h.movement_state {
        0 | 1 | 5 => update_locomotion_constraints(h, bodies, map, ticks),
        2 => slide_simulation(h, bodies),
        3 => step_locomotion_ik(h, bodies, map),
        // TODO: walk_simulation (4) drives a human seated in a vehicle
        _ => {}
    }
}
