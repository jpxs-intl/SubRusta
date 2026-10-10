use glam::Vec3;
use rosa_physics::{Bond, Joint as BondJoint, RigidBodies, RotMatrix, Table, body::RigidBodyType, rotation::IDENTITY};
use rosa_protocol::CharacterCustomization;

use super::{
    AttachmentChain, AttachmentGroup, AttachmentLink, Attachments, BONE_COUNT, Bone, Human, INVENTORY_SLOTS, InventorySlot, Joint, Locomotion, QueuedAction,
    bones::{BONES, BoneTemplate},
};
use crate::{player::Player, rng::rand};

const PLAYER_FACE_SHAPE: [Vec3; 4] = [Vec3::new(0.046875, 0.046875, -0.15), Vec3::ONE, Vec3::new(-0.046875, 0.046875, -0.15), Vec3::ONE];
const JOINT_ORDER: [usize; 15] = [1, 2, 3, 10, 11, 12, 13, 14, 15, 4, 5, 6, 7, 8, 9];
const LINKS: [(usize, usize); 13] = [(0, 1), (1, 2), (2, 3), (0, 4), (4, 5), (0, 7), (7, 8), (2, 10), (10, 11), (11, 12), (2, 13), (13, 14), (14, 15)];

pub fn create_human(humans: &mut Table<Human>, bodies: &mut RigidBodies, pos: Vec3, rot: &RotMatrix, player: Option<&Player>) -> Option<usize> {
    let id = humans.vacant()?;
    let spawn_protection = (rand() as i32 % 4 + 3) * 3600;

    let mut bones = Vec::with_capacity(BONE_COUNT);
    for (i, t) in BONES.iter().enumerate() {
        let o = BoneTemplate::rest_offset(i);
        let [r0, r1, r2] = *rot;
        let world = Vec3::new(
            ((r0.x * o.x + r1.x * o.y) + r2.x * o.z) + pos.x,
            ((r0.y * o.x + r1.y * o.y) + r2.y * o.z) + pos.y,
            ((r0.z * o.x + r1.z * o.y) + r2.z * o.z) + pos.z,
        );
        let inv_inertia = t.inv_inertia();
        let inertia = Vec3::new(1.0 / inv_inertia.x, 1.0 / inv_inertia.y, 1.0 / inv_inertia.z);
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
            margin: t.margin,
            half_extents: t.half_extents,
            mass: t.mass,
            inertia,
            inv_inertia,
            min_inertia,
            unk_rot: IDENTITY,
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
        let anchor_b = Vec3::new(t.joint.x - t.offset.x, t.joint.y - t.offset.y, t.joint.z - t.offset.z);
        let Some(bond) = bodies.create_bond(Bond::Joint(BondJoint::new(bones[t.parent].body, bones[child].body, t.joint, anchor_b, id as i32))) else { continue };
        joints.push(Joint { bond, parent: t.parent, child });
        bones[child].joint = Some(bond);
    }

    let mut attachments = build_attachments(&bones);
    update_attachment_centroids(&mut attachments, bodies);
    update_attachment_centroids(&mut attachments, bodies);

    let (team, customization, face_shape) = match player {
        Some(p) => (Some(p.team), p.customization, PLAYER_FACE_SHAPE),
        None => (None, bot_customization(), bot_face_shape()),
    };

    let human = Human {
        player: player.map(|p| p.player_id),
        account: player.map(|p| p.account_id),
        stocks: 0,
        team,
        customization,
        face_shape,
        health: 100,
        blood_level: 100,
        old_health: 100,
        chest_hp: 100,
        head_hp: 100,
        left_arm_hp: 100,
        right_arm_hp: 100,
        left_leg_hp: 100,
        right_leg_hp: 100,
        stamina: 127,
        max_stamina: 255,
        vehicle: None,
        seat: 0,
        seat_exit: 0,
        gear_x_input: 0.0,
        gear_y_input: 0.0,
        last_vehicle_cooldown: 0,
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
        unk_190: 0.0,
        unk_194: 0.0,
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
        despawn_ticks: 3600,
        last_vehicle: 0,
        strength: 0.0,
        eat_cooldown: 0,
        progress_bar: 0,
        down_timer: 0,
        view_turn: 0.0,
        body_yaw: 0.0,
        pitch_offset: 0.0,
        unk_128: 0.0,
        unk_12c: 0.0,
        look_yaw: 0.0,
        look_pitch: 0.0,
        client_body_yaw: 0.0,
        nearby_vehicles: Vec::new(),
        unk_b4: Vec3::ZERO,
        bleeding: false,
        unk_6adc: Vec3::ZERO,
        pos: Vec3::ZERO,
        aabb_min: Vec3::ZERO,
        aabb_max: Vec3::ZERO,
        grid_min: [0; 3],
        grid_max: [0; 3],
        bones,
        joints,
        attachments,
        locomotion: Locomotion::default(),
    };
    humans.insert(human)
}

fn bot_customization() -> CharacterCustomization {
    CharacterCustomization { gender: 0, head: 0, skin: 0, hair_color: 0, hair_style: 0, eye_color: 0, model: 0, necklace: 0, suit_color: 0, tie_color: 0 }
}

fn bot_face_shape() -> [Vec3; 4] {
    let x = (rand() % 16) as f32 * 0.125 * 0.25 * 0.0625;
    let s = (rand() % 16) as f32 * 0.125 * 0.0625 + 1.0;
    [Vec3::new(0.0625 - x, 0.03125, -0.15), Vec3::new(s, s, 1.0), Vec3::new(x - 0.0625, 0.03125, -0.15), Vec3::new(s, s, 1.0)]
}

fn build_attachments(bones: &[Bone]) -> Attachments {
    let body = |b: usize| bones[b].body;
    let anchor = |child: usize| {
        let t = &BONES[child];
        let local = Vec3::new(t.joint.x - t.offset.x, t.joint.y - t.offset.y, t.joint.z - t.offset.z);
        vec![(body(t.parent), t.joint), (body(child), local)]
    };
    let knee = |b: usize| vec![(body(b), Vec3::new(0.0, 0.25, 0.0))];
    let center = |b: usize| vec![(body(b), Vec3::ZERO)];
    let groups: Vec<AttachmentGroup> = [center(0), center(1), center(2), center(3), anchor(10), anchor(11), knee(11), anchor(13), anchor(14), knee(14), anchor(4), anchor(5), anchor(6), anchor(7), anchor(8), anchor(9)]
        .into_iter()
        .map(|points| AttachmentGroup { points, ..Default::default() })
        .collect();

    let links: Vec<AttachmentLink> = LINKS
        .iter()
        .map(|&(a, b)| {
            let mut shared_body = groups[b].points[0].0;
            for p in &groups[a].points {
                for q in &groups[b].points {
                    if p.0 == q.0 {
                        shared_body = p.0;
                    }
                }
            }
            AttachmentLink { a, b, shared_body }
        })
        .collect();

    let mut chains = Vec::new();
    for i in 0..links.len() {
        for j in i + 1..links.len() {
            let (li, lj) = (links[i], links[j]);
            for (k, s) in [li.a, li.b].into_iter().enumerate() {
                let end = if k == 0 { li.b } else { li.a };
                if s == lj.a {
                    chains.push(AttachmentChain { end, middle: lj.a, other: lj.b, body_a: li.shared_body, body_b: lj.shared_body });
                }
                if s == lj.b {
                    chains.push(AttachmentChain { end, middle: lj.b, other: lj.a, body_a: li.shared_body, body_b: lj.shared_body });
                }
            }
        }
    }

    Attachments { groups, links, chains }
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
        g.centroid = Vec3::new(sum.x * inv, sum.y * inv, sum.z * inv);
    }
    // TODO: the timers that follow the group table (64 entries) are decremented here
}
