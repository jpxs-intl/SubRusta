use glam::Vec3;
use rosa_math::vector::Vector;
use rosa_protocol::clientbound::game::events::{Event, ServerEvent, explosion::EventExplosion};

use crate::{PlayerId, input::ITEM_SECONDARY, items::state::ItemState, sim::Sim};

const GRENADE_KILL_RANGE: f32 = 4.0;
const GRENADE_PUSH_RANGE: f32 = 8.0;
const PUSH_FALLOFF: f32 = 3.0 / 32.0;
const PUSH_STRENGTH: f32 = 1.5;
const MIN_PUSH_WEIGHT: f32 = 0.125;
/// The damage a grenade kill scores for the primer's criminal rating and team kill punishment.
const GRENADE_SCORE: i32 = 100;

impl Sim {
    /// A grenade: the secondary key pulls the pin (or puts it back while still held); thrown, it counts down from
    /// 239 and blows up.
    pub(crate) fn grenade_logic(&mut self, id: usize) {
        let item = self.items.get_mut(id).unwrap();
        let (input, last, holder) = (item.input_flags, item.last_input_flags, item.parent_human);

        let rising = input & ITEM_SECONDARY != 0 && last & ITEM_SECONDARY == 0;

        let holder_player = (holder != -1).then(|| self.humans.get(holder as usize)).flatten().and_then(|h| h.player);

        let ItemState::Grenade { pin, fuse, primer } = &mut item.state else { return };

        if *pin > 0 {
            if rising {
                *fuse = 240;
                *pin = 0;
                if holder != -1 {
                    *primer = holder_player;
                }
            }

            return;
        }

        let c = *fuse;

        if c <= 0 {
            return;
        }

        if c == 240 {
            if rising {
                *pin = 1;
                *primer = None;
            } else if holder == -1 {
                *fuse = 240 - 1;
            }
            return;
        }

        *fuse = c - 1;
        if c - 1 != 1 {
            return;
        }

        item.despawn_time = 0;
        let pos = self.bodies.get(item.body).map_or(item.pos2, |b| b.pos);
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::Explosion(EventExplosion { size: 0, pos: Vector(pos) }) });
        let primer = *primer;
        self.grenade_explosion(pos, primer);
    }

    /// grenade_explosion: kills every human whose pelvis is within 4 of it and throws the bones of everyone within 8
    /// outwards. The primer is scored for each player killed other than themselves.
    fn grenade_explosion(&mut self, at: Vec3, primer: Option<PlayerId>) {
        let mut killed = Vec::new();
        for (_, human) in self.humans.iter_mut() {
            let dist = human.bones[0].pos.distance(at);

            if dist < GRENADE_KILL_RANGE {
                human.health.vitality = 0;
                killed.extend(human.player);
            }

            if dist < GRENADE_PUSH_RANGE {
                for bone in &human.bones {
                    if let Some(body) = self.bodies.get_mut(bone.body) {
                        body.vel += blast_push(body.pos - at, bone.mass);
                    }
                }
            }
        }

        let Some(primer) = primer else { return };

        for victim in killed.into_iter().filter(|&v| v != primer) {
            if self.same_team(primer, victim) {
                self.punish_team_kill(primer, GRENADE_SCORE);
            }

            self.handle_criminal_rating(primer, victim, GRENADE_SCORE);
        }
    }
}

/// The velocity a blast gives a bone `offset` away from it: outwards, growing with the square of how far inside the
/// push range it is, and less for heavier bones (but never under an eighth).
fn blast_push(offset: Vec3, mass: f32) -> Vec3 {
    let dist = offset.length();
    let weight = (GRENADE_PUSH_RANGE / (mass + GRENADE_PUSH_RANGE)).max(MIN_PUSH_WEIGHT);
    let falloff = (GRENADE_PUSH_RANGE - dist) * PUSH_FALLOFF;
    offset.normalize_or_zero() * (falloff * falloff * PUSH_STRENGTH * weight)
}
