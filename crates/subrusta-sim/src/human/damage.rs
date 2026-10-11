use glam::Vec3;

use super::{Human, bones::BONES};
use crate::{human::BoneId, world::capsule::segment_closest_points};

/// Where a ray met a bone capsule.
pub struct BoneHit {
    pub bone: BoneId,
    pub pos: Vec3,
    pub normal: Vec3,
    pub fraction: f32,
}

/// trace_ray_human: the first bone capsule (each bone's shape axis, length and radius) the segment `start..end`
/// passes within `radius` of.
pub fn trace_ray_human(h: &Human, start: Vec3, end: Vec3, radius: f32) -> Option<BoneHit> {
    let mut best: Option<BoneHit> = None;
    let mut fraction = 1.0f32;

    for (i, (bone, template)) in h.bones.iter().zip(BONES.iter()).enumerate() {
        let axis = bone.rot[template.shape as usize];
        let (length, thickness) = (template.shape_size[0], template.shape_size[1]);
        let (back, front) = (-length * 0.5, length * 0.5);

        let p = bone.pos;

        let q0 = back * axis + p;
        let q1 = p + axis * front;

        let (hit, on_ray, on_bone, _) = segment_closest_points(start, end, q0, q1, radius + thickness);

        if !hit {
            continue;
        }

        let d = on_bone - on_ray;
        let len = d.length();

        let normal = if len != 0.0 {
            let inv = 1.0 / len;
            d * inv
        } else {
            Vec3::ZERO
        };

        let e = end - start;
        let t = ((on_ray.x - start.x) * e.x + (on_ray.y - start.y) * e.y + (on_ray.z - start.z) * e.z) / e.length_squared();

        if !(fraction <= t) {
            fraction = t;
            best = Some(BoneHit { bone: BoneId::ALL[i], pos: on_ray, normal, fraction: t });
        }
    }
    best.filter(|b| 1.0 > b.fraction)
}

/// damage_human: a hit on a bone. The torso loses chest health and adds pain, the head loses head health and health
/// directly, arms lose three times the damage and legs twice; torso and leg hits cost health the more blood is
/// already gone, and any hit may start bleeding.
pub fn damage_human(h: &mut Human, bone: BoneId, damage: i32) {
    if !h.is_immortal {
        h.health.blood_level -= damage / 2;
    }
    let blood_health = |health: i32, blood: i32| (health as f32 - ((90 - blood) * damage) as f32 / 100.0) as i32;
    match bone {
        BoneId::Pelvis | BoneId::Stomach | BoneId::Torso => {
            h.health.chest -= damage;
            h.pain += damage;
            if h.health.blood_level < 100 {
                let scale = (1.0f32).min((100 - h.health.blood_level) as f32 / 25.0);
                h.health.health = (h.health.health as f32 - scale * damage as f32) as i32;
            }
        }
        BoneId::Head => {
            h.health.head -= damage;
            h.health.health -= damage;
        }
        BoneId::ShoulderLeft | BoneId::ForearmLeft | BoneId::HandLeft => h.health.left_arm += damage - damage * 4,
        BoneId::ShoulderRight | BoneId::ForearmRight | BoneId::HandRight => h.health.right_arm += damage - damage * 4,
        BoneId::ThighLeft | BoneId::ShinLeft | BoneId::FootLeft => {
            h.health.left_leg -= damage * 2;

            if h.health.blood_level < 100 {
                h.health.health = blood_health(h.health.health, h.health.blood_level);
            }
        }
        _ => {
            h.health.right_leg -= damage * 2;

            if h.health.blood_level < 100 {
                h.health.health = blood_health(h.health.health, h.health.blood_level);
            }
        }
    }

    if damage > 0 && (rand::random_range(0..32) as i32) < damage {
        h.bleeding = true;
    }

    h.pain = h.pain.min(90);
    h.health.health = h.health.health.max(0);
}
