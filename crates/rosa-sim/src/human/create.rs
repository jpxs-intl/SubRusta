use glam::Vec3;
use rosa_physics::{Bond, Joint as BondJoint, RigidBodies, RotMatrix, Table, body::RigidBodyType};
use rosa_protocol::CharacterCustomization;

use super::{
    AttachmentGroup, Attachments, BONE_COUNT, Bone, Human, INVENTORY_SLOTS, InventorySlot, Joint, Locomotion, QueuedAction,
    bones::{BONES, BoneTemplate},
};
use crate::{human::BoneId, player::Player};
use crate::{human::HumanHealth, time::TICKS_PER_MINUTE};

const JOINT_ORDER: [usize; 15] = [1, 2, 3, 10, 11, 12, 13, 14, 15, 4, 5, 6, 7, 8, 9];

pub fn create_human(humans: &mut Table<Human>, bodies: &mut RigidBodies, pos: Vec3, rot: &RotMatrix, player: Option<&Player>) -> Option<usize> {
    let id = humans.vacant()?;
    let spawn_protection = (rand::random_range(0..4) + 3) * TICKS_PER_MINUTE;

    let mut bones = Vec::with_capacity(BONE_COUNT);
    for (i, t) in BONES.iter().enumerate() {
        let o = BoneTemplate::rest_offset(i);
        let [r0, r1, r2] = *rot;
        let world = r0 * o.x + r1 * o.y + r2 * o.z + pos;
        let inv_inertia = t.inv_inertia();
        let inertia = 1.0 / inv_inertia;
        let body = bodies.create(RigidBodyType::HumanBone, world, *rot, None, inertia, t.mass)?;
        if let Some(b) = bodies.get_mut(body) {
            b.owner = id as i32;
            b.part = i as i32;
        }
        let mut min_inertia = inertia.x;
        if !(min_inertia <= inertia.y) {
            min_inertia = inertia.y;
        }
        if !(min_inertia <= inertia.z) {
            min_inertia = inertia.z;
        }
        bones.push(Bone {
            body,
            pos: world,
            pos2: world,
            vel: Vec3::ZERO,
            rot: *rot,
            ang_vel: Vec3::ZERO,
            mass: t.mass,
            min_inertia,
            angles: Vec3::ZERO,
            networked_rot: [Vec3::ZERO; 3],
            limit_angles: [0.0; 3],
            capsule: [Vec3::ZERO; 2],
            joint: None,
            ground_contact: 0,
            strength: 1.0,
        });
    }

    let mut joints = Vec::with_capacity(JOINT_ORDER.len());
    for child in JOINT_ORDER {
        let t = &BONES[child];
        let anchor_b = t.joint - t.offset;
        let Some(bond) = bodies.create_bond(Bond::Joint(BondJoint::new(bones[t.parent as usize].body, bones[child].body, t.joint, anchor_b, id as i32))) else {
            continue;
        };
        joints.push(Joint { bond });
        bones[child].joint = Some(bond);
    }

    let mut attachments = build_attachments(&bones);
    update_attachment_centroids(&mut attachments, bodies);
    update_attachment_centroids(&mut attachments, bodies);

    let (team, customization) = match player {
        Some(p) => (Some(p.team), p.customization),
        None => (None, bot_customization()),
    };

    let human = Human {
        player: player.map(|p| p.player_id),
        account: player.map(|p| p.account_id),
        stocks: 0,
        team,
        customization,
        health: HumanHealth::default(),
        stamina: 127,
        max_stamina: 255,
        vehicle: None,
        seat: 0,
        seat_exit: 0,
        gear_x_input: 0.0,
        gear_y_input: 0.0,
        last_vehicle_cooldown: 0,
        traffic_driver: false,
        is_immortal: false,
        is_on_ground: false,
        spawn_protection,
        input_flags: 0,
        last_input_flags: 960,
        movement_state: 1,
        getup_ticks: 0,
        movement_mode: 0,
        is_standing: false,
        view_yaw: 0.0,
        view_pitch: 0.0,
        yaw_offset: 0.0,
        strafe_input: 0.0,
        walk_input: 0.0,
        stance: 1.0,
        lean_forward: 0.0,
        lean_side: 0.0,
        pain: 0,
        jump_spin_yaw: 0.0,
        jump_spin_pitch: 0.0,
        free_look_yaw: 0.0,
        free_look_pitch: 0.0,
        hand_sway: Vec3::ZERO,
        hand_sway_vel: Vec3::ZERO,
        inventory: [InventorySlot::default(); INVENTORY_SLOTS],
        actions: [QueuedAction::default(); 8],
        actions_queued: 0,
        actions_finished: 0,
        action_type: 0,
        action_progress: 0.0,
        action_duration: 0,
        action_hand: 0,
        action_slot: 0,
        throw_pitch: 0.0,
        despawn_ticks: TICKS_PER_MINUTE,
        last_vehicle: 0,
        strength: 0.0,
        eat_cooldown: 0,
        progress_bar: 0,
        down_timer: 0,
        view_turn: 0.0,
        body_yaw: 0.0,
        pitch_offset: 0.0,
        look_yaw: 0.0,
        look_pitch: 0.0,
        client_body_yaw: 0.0,
        nearby_vehicles: Vec::new(),
        bleeding: false,
        pos: Vec3::ZERO,
        aabb_min: Vec3::ZERO,
        aabb_max: Vec3::ZERO,
        grid_min: [0; 3],
        grid_max: [0; 3],
        bones,
        joints,
        locomotion: Locomotion::default(),
    };
    humans.insert(human)
}

fn bot_customization() -> CharacterCustomization {
    CharacterCustomization { gender: 0, head: 0, skin: 0, hair_color: 0, hair_style: 0, eye_color: 0, model: 0, necklace: 0, suit_color: 0, tie_color: 0 }
}

fn build_attachments(bones: &[Bone]) -> Attachments {
    let body = |b: BoneId| bones[b as usize].body;
    let anchor = |child: BoneId| {
        let t = &BONES[child as usize];
        let local = t.joint - t.offset;
        vec![(body(t.parent), t.joint), (body(child), local)]
    };
    let knee = |b: BoneId| vec![(body(b), Vec3::new(0.0, 0.25, 0.0))];
    let center = |b: BoneId| vec![(body(b), Vec3::ZERO)];
    let groups: Vec<AttachmentGroup> = [
        center(BoneId::Pelvis),
        center(BoneId::Stomach),
        center(BoneId::Torso),
        center(BoneId::Head),
        anchor(BoneId::ThighLeft),
        anchor(BoneId::ShinLeft),
        knee(BoneId::ShinLeft),
        anchor(BoneId::ThighRight),
        anchor(BoneId::ShinRight),
        knee(BoneId::ShinRight),
        anchor(BoneId::ShoulderLeft),
        anchor(BoneId::ForearmLeft),
        anchor(BoneId::HandLeft),
        anchor(BoneId::ShoulderRight),
        anchor(BoneId::ForearmRight),
        anchor(BoneId::HandRight),
    ]
    .into_iter()
    .map(|points| AttachmentGroup { points, ..Default::default() })
    .collect();

    Attachments { groups }
}

pub fn update_attachment_centroids(attachments: &mut Attachments, bodies: &RigidBodies) {
    for g in &mut attachments.groups {
        g.prev_centroid = g.centroid;
        let mut sum = Vec3::ZERO;
        for &(id, l) in &g.points {
            let Some(b) = bodies.get(id) else { continue };
            let ([r0, r1, r2], p) = (b.rot, b.pos);
            sum.x += ((r0.x * l.x + p.x) + r1.x * l.y) + r2.x * l.z;
            sum.y += ((r0.y * l.x + p.y) + r1.y * l.y) + r2.y * l.z;
            sum.z += ((r0.z * l.x + p.z) + r1.z * l.y) + r2.z * l.z;
        }
        let inv = 1.0 / g.points.len() as f32;
        g.centroid = sum * inv;
    }
}
