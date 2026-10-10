use glam::{IVec3, Vec3};
use rosa_physics::{Bond, RigidBodies};
use rosa_protocol::clientbound::game::events::sound::Sound;

use super::{
    BONE_COUNT, Human,
    bones::{BONES, BoneId},
};
use crate::{
    items::Touchables,
    world::{
        capsule::{CapsuleHit, capsule_intersect_level, segment_closest_points},
        map::Map,
        trace::line_intersect_level,
    },
};

const ANTI_TUNNEL_BONES: usize = 4;
const BONE_FRICTION: f32 = 0.8;
const BONE_FRICTION_STATE_2: f32 = 0.1;
const BONE_SOFTNESS: f32 = 0.0625;
const HEAD_IMPACT: f32 = 4.0 / 15.0;
const TORSO_IMPACT: f32 = 0.2;
const SPIN_LIMIT: f32 = 1.0 / 4096.0;
const LIMIT_STIFFNESS: f32 = 0.25;
const LIMIT_DAMPING: f32 = 0.5;
const ITEM_FRICTION: f32 = 0.4;
const ITEM_DEPTH_SCALE: f32 = 0.0625;
const ITEM_SOFTNESS: f32 = 0.125;
const MAX_NEARBY_VEHICLES: usize = 8;
const VEHICLE_FRICTION: f32 = 0.4;
const VEHICLE_DEPTH_SCALE: f32 = 0.03125;
const VEHICLE_SOFTNESS: f32 = 0.0625;
/// How fast a vehicle's point of contact must move into a bone to hurt (legs take a third of a unit a tick).
const RUN_OVER: f32 = 0.2;
const LEG_RUN_OVER: f32 = 1.0 / 3.0;
const NO_ITEM_CONTACT: [BoneId; 4] = [BoneId::ForearmLeft, BoneId::HandLeft, BoneId::ForearmRight, BoneId::HandRight];

/// human_update_bones_bbox_wake_items: pull every bone's state from its body and rebuild the bone capsules and bounds.
pub fn sync_bones(h: &mut Human, bodies: &mut RigidBodies, map: &Map) {
    h.aabb_min = Vec3::splat(65536.0);
    h.aabb_max = Vec3::splat(-65536.0);
    for (j, bone) in h.bones.iter_mut().enumerate() {
        let Some(body) = bodies.get_mut(bone.body) else { continue };
        bone.pos2 = bone.pos;
        bone.vel = body.vel;
        bone.rot = body.rot;
        bone.pos = body.pos;
        bone.ang_vel = body.ang_vel;
        if j < ANTI_TUNNEL_BONES
            && let Some(hit) = line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, bone.pos2, bone.pos)
        {
            let t = 0.975 * hit.hit.fraction;
            let (p, q) = (bone.pos, bone.pos2);
            let clamped = (p - q) * t + q;
            body.pos = clamped;
            bone.pos = clamped;
        }

        let t = &BONES[j];
        let row = bone.rot[t.shape as usize];
        let (half, neg_half) = (t.shape_size[0] * 0.5, -t.shape_size[0] * 0.5);
        let p = bone.pos;
        let a = p + row * neg_half;
        let b = row * half + p;
        bone.capsule = [a, b];
        let r = t.shape_size[1];
        let (mut mn, mut mx) = (h.aabb_min.to_array(), h.aabb_max.to_array());
        for k in 0..3 {
            for end in [a, b] {
                let v = end.to_array()[k];
                let lo = v - r;
                if !(mn[k] <= lo) {
                    mn[k] = lo;
                }
                let hi = v + r;
                if !(hi <= mx[k]) {
                    mx[k] = hi;
                }
            }
        }
        h.aabb_min = Vec3::from_array(mn);
        h.aabb_max = Vec3::from_array(mx);
    }
    h.aabb_min.y -= 0.25;
    for k in 0..3 {
        h.grid_min[k] = (h.aabb_min.to_array()[k] * 0.25) as i32;
        h.grid_max[k] = (h.aabb_max.to_array()[k] * 0.25) as i32;
    }
}

/// The end of human_simulation: every bone's rotation relative to its parent (sent to clients) and its Euler angles.
pub(super) fn update_networked_bones(h: &mut Human) {
    for child in 1..BONE_COUNT {
        let [p0, p1, p2] = h.bones[BONES[child].parent.index()].rot;
        let [c0, c1, c2] = h.bones[child].rot;
        let m = [Vec3::new(c0.dot(p0), c0.dot(p1), c0.dot(p2)), Vec3::new(c1.dot(p0), c1.dot(p1), c1.dot(p2)), Vec3::new(c2.dot(p0), c2.dot(p1), c2.dot(p2))];
        let a0 = (m[2].y as f64).atan2(m[2].z as f64) as f32;
        let r = (m[1].x * m[1].x + m[0].x * m[0].x).sqrt();
        let a1 = ((-m[2].x) as f64).atan2(r as f64) as f32;
        let (s, c) = (a0 as f64).sin_cos();
        let (s, c) = (s as f32, c as f32);
        let a2 = ((s * m[0].z - c * m[0].y) as f64).atan2((m[1].y * c - m[1].z * s) as f64) as f32;
        let bone = &mut h.bones[child];
        bone.angles = Vec3::new(a0, a1, a2);
        bone.networked_rot = m;
    }
}

pub(super) fn bone_world_contacts(h: &mut Human, bodies: &mut RigidBodies, map: &mut Map, out: &mut Vec<HumanOutput>) {
    let movement_state = h.movement_state;
    for j in 0..BONE_COUNT {
        if j == BoneId::FootLeft as usize || j == BoneId::FootRight as usize {
            continue;
        }
        let standing = movement_state != 2 && h.health.vitality > 0 && h.health.health > 49;
        if (j == BoneId::ShinLeft as usize || j == BoneId::ShinRight as usize) && standing {
            continue;
        }
        let depth_scale = if j == BoneId::Head as usize {
            0.0625
        } else if j <= BoneId::Torso as usize {
            0.03125
        } else {
            1.0 / 64.0
        };

        let t = &BONES[j];
        let bone = &h.bones[j];
        let row = bone.rot[t.shape as usize];
        let size = t.shape_size[0];
        let neg = -size;
        let p = bone.pos;
        let nh = neg * 0.5;
        let mut a = nh * row + p;
        if movement_state == 0 && h.health.vitality > 0 && h.health.health > 49 && (j == BoneId::ThighLeft as usize || j == BoneId::ThighRight as usize) {
            let q = neg * 0.125;
            a = q * row + p;
        }
        let half = size * 0.5;
        let b = row * half + p;
        let radius = t.shape_size[1];

        let Some(hit) = capsule_intersect_level(&map.ground, &map.level.area, &map.level.meshes, a, b, radius) else { continue };
        let n = hit.normal;
        let impact = bone.vel.dot(n).abs();
        if !(impact <= 1.0 / 15.0) && hit.area != -1 && hit.block.x != -1 {
            add_bullet_hole(map, hit, bone.pos, bone.vel, out);
        }
        if h.last_vehicle_cooldown == 0 && !h.is_immortal {
            if j == BoneId::Head as usize {
                if impact > HEAD_IMPACT {
                    h.health.vitality -= 120;
                }
            } else if j == BoneId::Pelvis as usize || j == BoneId::Torso as usize {
                if impact > TORSO_IMPACT {
                    h.health.vitality -= 120;
                }
            } else if (j == BoneId::ThighLeft as usize || j == BoneId::ThighRight as usize) && impact > HEAD_IMPACT {
                if j == BoneId::ThighLeft as usize {
                    h.health.left_leg = 0;
                } else {
                    h.health.right_leg = 0;
                }
            }
        }
        let friction = if movement_state == 2 { BONE_FRICTION_STATE_2 } else { BONE_FRICTION };
        let body = h.bones[j].body;
        let Some(bp) = bodies.get(body).map(|b| b.pos) else { continue };
        let offset = hit.pos - bp;
        if hit.set.is_none() {
            bodies.add_world_contact(body, offset, n, radius - hit.dist, friction, depth_scale, BONE_SOFTNESS);
        }
        h.is_on_ground = true;
    }
}

const TRACK_FRICTION: f32 = 0.4;
const TRACK_DEPTH_SCALE: f32 = 0.03125;
const TRACK_SOFTNESS: f32 = 0.0625;

/// The track part of human_generate_world_item_self_contacts: every bone capsule but the feet (and the shins of a
/// human standing) against the track walls within the human's bounds, its deepest-axis triangle pushing it out.
pub(super) fn bone_track_contacts(h: &Human, bodies: &mut RigidBodies, track: &crate::world::track::Track) {
    let tris = track.collect(h.aabb_min, h.aabb_max, crate::world::track::WALLS);
    if tris.is_empty() {
        return;
    }
    let standing = h.movement_state != 2 && h.health.vitality > 0 && h.health.health > 49;
    for (j, bone) in h.bones.iter().enumerate() {
        if j == BoneId::FootLeft as usize || j == BoneId::FootRight as usize {
            continue;
        }
        if (j == BoneId::ShinLeft as usize || j == BoneId::ShinRight as usize) && standing {
            continue;
        }
        let radius = BONES[j].shape_size[1];
        let [start, end] = bone.capsule;
        let mut best: Option<(Vec3, Vec3, f32)> = None;
        let mut best_d = 65536.0f32;
        for &[a, b, c] in &tris {
            if let Some(hit) = crate::world::capsule::capsule_intersect_triangle(start, end, a, b, c, radius)
                && !(best_d <= hit.2)
            {
                best_d = hit.2;
                best = Some(hit);
            }
        }
        let Some((p, n, d)) = best else { continue };
        let Some(bp) = bodies.get(bone.body).map(|b| b.pos) else { continue };
        let offset = p - bp;
        bodies.add_world_contact(bone.body, offset, n, radius - d, TRACK_FRICTION, TRACK_DEPTH_SCALE, TRACK_SOFTNESS);
    }
}

/// The item part of human_generate_world_item_self_contacts: every bone capsule but the forearms and hands against the hull of each nearby item.
pub(super) fn bone_item_contacts(h: &Human, bodies: &mut RigidBodies, touch: &mut Touchables) {
    for id in touch.grid.query(IVec3::from_array(h.grid_min), IVec3::from_array(h.grid_max)) {
        let Some(item) = touch.items.get(id) else { continue };
        let (mn, mx) = (item.aabb_min, item.aabb_max);
        if mn.x > h.aabb_max.x || h.aabb_min.x > mx.x || mn.z > h.aabb_max.z || h.aabb_min.z > mx.z || mn.y > h.aabb_max.y || h.aabb_min.y > mx.y {
            continue;
        }
        let Some(hull) = &touch.types[item.item_type as usize].hull else { continue };
        let item_body = item.body;
        let Some((item_pos, world)) = bodies.get(item_body).map(|b| (b.pos, hull.world_verts(b.pos, &b.rot))) else { continue };
        let mut woke = false;
        for (j, bone) in h.bones.iter().enumerate() {
            if NO_ITEM_CONTACT.iter().any(|&b| b as usize == j) {
                continue;
            }
            let radius = BONES[j].shape_size[1];
            let [start, end] = bone.capsule;
            let Some((p, n, d)) = hull.intersect_capsule(&world, start, end, radius) else { continue };
            let Some(bp) = bodies.get(bone.body).map(|b| b.pos) else { continue };
            let off_a = p - bp;
            let off_b = p - item_pos;
            bodies.add_body_contact(bone.body, item_body, off_a, off_b, n, radius - d, ITEM_FRICTION, ITEM_DEPTH_SCALE, ITEM_SOFTNESS);
            woke = true;
        }
        if woke && let Some(item) = touch.items.get_mut(id) {
            item.physics_settled = false;
            item.settled_timer = 0;
        }
    }
}

/// human_simulation's scan for the vehicles whose bounds overlap the human's, at most 8 in vehicle order.
pub(super) fn find_nearby_vehicles(h: &mut Human, touch: &Touchables) {
    let (lo, hi) = (h.aabb_min, h.aabb_max);
    h.nearby_vehicles = touch
        .vehicles
        .iter()
        .filter(|(_, v)| {
            let (mn, mx) = (v.bounds_min, v.bounds_max);
            !(mn.x > hi.x || lo.x > mx.x || mn.z > hi.z || lo.z > mx.z || mn.y > hi.y || lo.y > mx.y)
        })
        .map(|(id, _)| id)
        .take(MAX_NEARBY_VEHICLES)
        .collect();
}

/// human_collide_vehicle: every bone capsule but the feet against each nearby vehicle's chassis, pushing the two
/// apart; a vehicle the human was not last hit by hurts it when its point of contact moves into the bone fast
/// enough.
pub(super) fn vehicle_contacts(h: &mut Human, bodies: &mut RigidBodies, touch: &Touchables, out: &mut Vec<HumanOutput>) {
    for k in 0..h.nearby_vehicles.len() {
        let vid = h.nearby_vehicles[k];
        let Some(v) = touch.vehicles.get(vid) else { continue };
        let Some(t) = touch.vehicle_types.get(v.kind as usize) else { continue };
        for j in 0..BONE_COUNT {
            if j == BoneId::FootLeft as usize || j == BoneId::FootRight as usize {
                continue;
            }
            let shin = j == BoneId::ShinLeft as usize || j == BoneId::ShinRight as usize;
            let shape = &BONES[j];
            let bone = &h.bones[j];
            let row = bone.rot[shape.shape as usize];
            let (len, p) = (shape.shape_size[0], bone.pos);
            let back = if shin { -len * 0.125 } else { 0.5 * -len };
            let start = back * row + p;
            let half = len * 0.5;
            let end = p + row * half;
            let radius = shape.shape_size[1];
            let Some((hit, n, dist)) = crate::vehicle::physics::capsule_intersect_vehicle(v, t, start, end, radius) else { continue };
            let (Some(bp), Some((vp, vvel, w))) = (bodies.get(bone.body).map(|b| b.pos), bodies.get(v.body).map(|b| (b.pos, b.vel, b.ang_vel))) else {
                continue;
            };
            let off_a = hit - bp;
            let r = hit - vp;
            bodies.add_body_contact(bone.body, v.body, off_a, r, n, radius - dist, VEHICLE_FRICTION, VEHICLE_DEPTH_SCALE, VEHICLE_SOFTNESS);
            let arm = [BoneId::ForearmLeft, BoneId::HandLeft, BoneId::ForearmRight, BoneId::HandRight].iter().any(|&b| b as usize == j);
            if h.last_vehicle == vid as i32 || arm {
                continue;
            }
            let point = r.cross(w) + vvel;
            let rel = point - bone.vel;
            let leg = shin || j == BoneId::ThighLeft as usize || j == BoneId::ThighRight as usize;
            let threshold = if leg { LEG_RUN_OVER } else { RUN_OVER };
            if h.is_immortal || (rel.dot(n)) <= threshold {
                continue;
            }
            h.health.vitality -= 120;
            if (-119..=0).contains(&h.health.vitality) {
                out.push(HumanOutput::Sound { sound: Sound::BodyHit, pos: h.pos, volume: 1.0, pitch: 0.5 });
                if v.last_driver != -1 {
                    out.push(HumanOutput::RunOver { driver: crate::PlayerId(v.last_driver as u32), victim: h.player });
                }
            }
        }
    }
}

pub(super) fn joint_limits(h: &mut Human, bodies: &mut RigidBodies) {
    for child in 1..BONE_COUNT {
        let parent = BONES[child].parent;
        let Some(bond) = h.bones[child].joint else { continue };
        let (correction, angles) = joint_limit_correction(h, parent, BoneId::ALL[child]);
        h.bones[child].limit_angles = angles;
        if let Some(Bond::Joint(j)) = bodies.bond_mut(bond) {
            j.target_ang_vel = Vec3::ZERO;
            j.limit_correction = correction;
            let len = correction.length();
            j.limit_active = len > 0.0;
            j.spin_limit = SPIN_LIMIT;
        }
    }
}

fn maxss(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

/// human_accumulate_joint_limit_correction: (world-space corrective spin, clamped joint angles).
pub(super) fn joint_limit_correction(h: &Human, parent: BoneId, child: BoneId) -> (Vec3, [f32; 3]) {
    let [p0, p1, p2] = h.bones[parent as usize].rot;
    let [c0, c1, c2] = h.bones[child as usize].rot;
    let (m00, m01, m02) = (c0.dot(p0), c0.dot(p1), c0.dot(p2));
    let (m10, m11, m12) = (c1.dot(p0), c1.dot(p1), c1.dot(p2));
    let (m20, m21, m22) = (c2.dot(p0), c2.dot(p1), c2.dot(p2));

    let a0 = (m12 as f64).atan2(m22 as f64) as f32;
    let r = (m01 * m01 + m00 * m00).sqrt();
    let a1 = ((-m02) as f64).atan2(r as f64) as f32;
    let (s, c) = (a0 as f64).sin_cos();
    let (s, c) = (s as f32, c as f32);
    let a2 = ((s * m20 - c * m10) as f64).atan2((m11 * c - m21 * s) as f64) as f32;

    let (wp, wc) = (h.bones[parent.index()].ang_vel, h.bones[child.index()].ang_vel);
    let dw = wp - wc;
    let local = [dw.dot(p0), dw.dot(p1), dw.dot(p2)];

    let t = &BONES[child.index()];
    let (lmin, lmax) = (t.limit_min.to_array(), t.limit_max.to_array());
    let mut angles = [a0, a1, a2];
    let mut corr = [0f32; 3];
    for k in 0..3 {
        let a = angles[k];
        if lmin[k] > a {
            let x = -(a - lmin[k]) * LIMIT_STIFFNESS - LIMIT_DAMPING * local[k];
            if x > 0.0 {
                corr[k] += x;
            }
        }
        let clamped = if a > lmax[k] {
            let x = -(a - lmax[k]) * LIMIT_STIFFNESS - local[k] * LIMIT_DAMPING;
            if 0.0 > x {
                corr[k] += x;
            }
            if lmin[k] > a { if lmin[k] > lmax[k] { lmax[k] } else { maxss(lmin[k], a) } } else { lmax[k] }
        } else {
            let m = maxss(lmin[k], a);
            if m > lmax[k] { lmax[k] } else { maxss(lmin[k], a) }
        };
        angles[k] = clamped;
    }

    let world = p1 * corr[1] + p0 * corr[0] + p2 * corr[2] + 0.0;
    (world, angles)
}

/// The parts of another human its bones are tested against: its bounds and each bone's capsule and body.
pub struct OtherHuman {
    pub aabb_min: Vec3,
    pub aabb_max: Vec3,
    pub bones: Vec<([Vec3; 2], usize)>,
}

impl OtherHuman {
    pub fn of(h: &Human) -> Self {
        Self { aabb_min: h.aabb_min, aabb_max: h.aabb_max, bones: h.bones.iter().map(|b| (b.capsule, b.body)).collect() }
    }
}

const NO_HUMAN_CONTACT: [usize; 4] = [11, 12, 14, 15];
const HUMAN_FRICTION: f32 = 0.4;
const HUMAN_DEPTH_SCALE: f32 = 1.0 / 32.0;
const HUMAN_SOFTNESS: f32 = 0.0625;

/// The human-against-human part of human_generate_world_item_self_contacts: every bone capsule (but the shins and
/// feet) against those of each later, unseated human whose bounds overlap, pushing the other human's bone away.
pub fn human_contacts(h: &Human, bodies: &mut RigidBodies, others: &[OtherHuman]) {
    let (mn, mx) = (h.aabb_min, h.aabb_max);
    for o in others {
        if o.aabb_min.x > mx.x || mn.x > o.aabb_max.x || o.aabb_min.z > mx.z || mn.z > o.aabb_max.z || o.aabb_min.y > mx.y || mn.y > o.aabb_max.y {
            continue;
        }
        for (b, bone) in h.bones.iter().enumerate() {
            if NO_HUMAN_CONTACT.contains(&b) {
                continue;
            }
            let [a0, a1] = bone.capsule;
            for (k, &([b0, b1], other_body)) in o.bones.iter().enumerate() {
                if NO_HUMAN_CONTACT.contains(&k) {
                    continue;
                }
                let radius = BONES[b].shape_size[1] + BONES[k].shape_size[1];
                let (hit, pa, pb, dist) = segment_closest_points(a0, a1, b0, b1, radius);
                if !hit {
                    continue;
                }
                let d = pb - pa;
                let n = d.normalize_or_zero();
                let mid = (pa + pb) * 0.5;
                let (Some(ob), Some(sb)) = (bodies.get(other_body), bodies.get(bone.body)) else { continue };
                let (op, sp) = (ob.pos, sb.pos);
                let off_other = mid - op;
                let off_self = mid - sp;
                bodies.add_body_contact(other_body, bone.body, off_other, off_self, n, radius - dist, HUMAN_FRICTION, HUMAN_DEPTH_SCALE, HUMAN_SOFTNESS);
            }
        }
    }
}

/// What a human's tick asks of the sim, in the order it happened.
pub enum HumanOutput {
    /// A glass pane broke (event 0x10).
    Glass(GlassBreak),
    /// The human lost blood: a blood drop (bullet-hit event, hit type 3) at this point.
    Blood(Vec3),
    /// The dead human lets go of its player, who settles their death (stocks, wealth tax) and gets an update-player
    /// event.
    ReleasePlayer(crate::PlayerId),
    /// The dead human of a player who has left taxes their account.
    TaxAccount(u32),
    /// A sound at a place (event 9), e.g. a magazine going in.
    Sound { sound: rosa_protocol::clientbound::game::events::sound::Sound, pos: Vec3, volume: f32, pitch: f32 },
    /// A vehicle's last driver killed the human by running it over.
    RunOver { driver: crate::PlayerId, victim: Option<crate::PlayerId> },
    /// A player let go of the mouse with a hand free: the head's look ahead (start to end) may open or close their
    /// corporation's garage door, if the human (at `pos`) is at its base.
    DoorProbe { player: Option<crate::PlayerId>, start: Vec3, end: Vec3, pos: Vec3 },
}

/// A breakable face of a level cell that broke this tick, for the bullet-hole event.
pub struct GlassBreak {
    pub area: i32,
    pub block: IVec3,
    pub cell: u32,
    pub face: i32,
    pub pos: Vec3,
    pub vel: Vec3,
}

/// add_bullet_hole: a hit on a breakable wall of a custom level shape (a glass pane) removes that wall from the cell.
pub fn add_bullet_hole(map: &mut Map, hit: CapsuleHit, pos: Vec3, vel: Vec3, out: &mut Vec<HumanOutput>) -> bool {
    if hit.area as u32 > 3 || hit.block.x < 0 {
        return false;
    }
    if hit.cell & 0xe000_0000 != 0x8000_0000 || hit.face_attr & 0x10000 == 0 {
        return false;
    }
    let face = hit.face_attr & 0xffff;
    out.push(HumanOutput::Glass(GlassBreak { area: hit.area, block: hit.block, cell: hit.cell, face: face as i32, pos, vel }));
    let b = hit.block;
    map.level.area.set_layer1(b.x, b.y, b.z, (0x10000u32 << (face & 31)) | hit.cell);
    true
}
