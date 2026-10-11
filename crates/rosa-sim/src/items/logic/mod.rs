use rosa_math::vector::Vector;
use rosa_protocol::clientbound::game::{
    GameState, ItemKind,
    events::{Event, ServerEvent, bullet::EventBullet, sound::Sound},
};

use super::Sim;
use crate::input::ITEM_USE;
use crate::time::TICKS_PER_MINUTE;
use crate::{
    PlayerId,
    items::{Item, state::ItemState},
    sim::bullets::{BULLET_DATA, create_bullet},
};
use crate::{human::arms::calculate_spread_vector, items::logic::phone::PhoneStatus};

pub mod bandage;
pub mod burger;
pub mod cash;
pub mod grenade;
pub mod key;
pub mod phone;

const NEVER_DESPAWN: i32 = 65535;
const GROUND_DESPAWN: i32 = 5 * TICKS_PER_MINUTE;

impl Sim {
    /// The per-item part of logic_item: despawn timers, then each type's behaviour, then the item's keys move to
    /// last tick's.
    pub(crate) fn item_behaviours(&mut self) {
        self.build_radio_channels();
        for id in self.items.ids() {
            self.item_despawn_rules(id);
            let Some(kind) = self.items.get(id).map(|i| i.item_type) else { continue };
            if self.item_types[kind as usize].is_gun {
                self.fire_gun(id);
            }
            if kind == ItemKind::Computer {
                self.with_computer(id, |sim, c| sim.logic_computer(id, c));
            }
            match kind {
                ItemKind::Key => self.key_logic(id),
                ItemKind::Radio => {
                    let pressed = self.items.get(id).is_some_and(|i| i.input_flags & ITEM_USE != 0);
                    if let Some(ItemState::Radio { transmitting, .. }) = self.items.get_mut(id).map(|i| &mut i.state) {
                        *transmitting = pressed;
                    }
                }
                ItemKind::Phone | ItemKind::PhonePay => self.phone_logic(id),
                ItemKind::Arcade => {
                    if let Some(ItemState::Arcade { frame, top_line }) = self.items.get_mut(id).map(|i| &mut i.state) {
                        *top_line = 64;
                        *frame = (*frame + 1) & 0xff;
                    }
                }
                ItemKind::Grenade if self.gamestate == GameState::InGame => self.grenade_logic(id),
                _ => {}
            }
            if !(kind == ItemKind::Grenade && self.gamestate != GameState::InGame) {
                self.use_logic(id);
            }
            if let Some(item) = self.items.get_mut(id) {
                item.last_input_flags = item.input_flags;
                item.input_flags = 0;
            }
        }
    }

    /// fire_gun: a held gun in play chambers a round from its magazine and fires while the trigger is held, every
    /// fire_rate ticks (once per pull for the 9mm and Magnum, the Magnum on the trigger's sixth tick). The shot
    /// leaves along the gun's barrel with spread, and the recoil pushes and turns the gun.
    fn fire_gun(&mut self, id: usize) {
        const FIRE_TICK: i32 = 2;
        const MAGNUM_FIRE_TICK: i32 = 6;
        const LEVER_BACK: f32 = -0.625;
        const LEVER_UP: f32 = 0.03125;
        let in_game = self.gamestate == GameState::InGame;
        let Some(item) = self.items.get(id) else { return };
        let (kind, holder, input, body_id) = (item.item_type, item.parent_human, item.input_flags, item.body);
        let mag = item.children.first().copied();
        let ty = &self.item_types[kind as usize];
        let (fire_rate, bullet_type, velocity, spread, gun_mass) = (ty.fire_rate, ty.bullet_type, ty.bullet_velocity, ty.bullet_spread, ty.mass);
        let item = self.items.get_mut(id).unwrap();
        let ItemState::Gun { rounds, cooldown, trigger_ticks } = &mut item.state else { return };
        if *cooldown > 0 {
            *cooldown -= 1;
        }
        if holder == -1 || !in_game {
            return;
        }
        let semi = matches!(kind, ItemKind::Magnum | ItemKind::Pistol);
        let fire_tick = if kind == ItemKind::Magnum { MAGNUM_FIRE_TICK } else { FIRE_TICK };
        if input & ITEM_USE != 0 {
            if kind == ItemKind::Magnum || *trigger_ticks <= 1 || semi {
                *trigger_ticks += 1;
            }
        } else {
            *trigger_ticks = 0;
            if semi {
                *cooldown = 0;
            }
        }
        let chamber = |items: &mut rosa_physics::Table<Item>| {
            let Some(m) = mag.and_then(|m| items.get_mut(m)) else { return false };
            if let ItemState::Stock { left } = &mut m.state
                && *left > 0
            {
                *left -= 1;
                return true;
            }
            false
        };

        if *rounds == 0
            && chamber(&mut self.items)
            && let Some(ItemState::Gun { rounds, .. }) = self.items.get_mut(id).map(|i| &mut i.state)
        {
            *rounds = 1;
        }

        let item = self.items.get_mut(id).unwrap();
        let ItemState::Gun { rounds, cooldown, trigger_ticks } = &mut item.state else { return };
        if *cooldown > 0 || *trigger_ticks != fire_tick || *rounds <= 0 {
            return;
        }
        let Some(body) = self.bodies.get(body_id) else { return };
        let ([_, r1, r2], p, bv) = (body.rot, body.pos, body.vel);
        let k = -velocity;
        let mut vel = r2 * k + bv;
        let s = calculate_spread_vector(&mut self.noise_seed, spread, 0.0);
        vel += s;
        let muzzle = r2 * 0.0 + p;
        let shooter = self.humans.get(holder as usize).and_then(|h| h.player);
        if create_bullet(&mut self.bullets, bullet_type, muzzle, vel, shooter, self.bodies.gravity_scale) {
            self.stats.bullets += 1;
        }
        let shown = p - bv;
        let e = EventBullet { bullet_type, item_id: id as i32, pos: Vector(shown), vel: Vector(vel) };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::Bullet(e) });
        let s = calculate_spread_vector(&mut self.noise_seed, spread + spread, 0.0);
        let r = vel + s;
        let bm = BULLET_DATA.get(bullet_type as usize).map_or(0.0, |d| d.0);
        let push = -((bm + bm) / gun_mass);
        let lever = r2 * LEVER_BACK + r1 * LEVER_UP;
        let turn = lever.cross(r);
        if let Some(body) = self.bodies.get_mut(body_id) {
            body.vel = push * r + body.vel;
            let l = body.ang_momentum;
            body.ang_momentum = turn * push + l;
        }
        *rounds -= 1;
        *cooldown = fire_rate;
        if *rounds == 1
            && chamber(&mut self.items)
            && let Some(ItemState::Gun { rounds, .. }) = self.items.get_mut(id).map(|i| &mut i.state)
        {
            *rounds = 1;
        }
    }

    /// Items held, or left inside a gun on the ground, never despawn; anything else lying around despawns after
    /// 18000 ticks (counted twice a tick with cleanup_items), except in round and eliminator.
    fn item_despawn_rules(&mut self, id: usize) {
        let modeless = self.rules.keep_items;
        let Some(item) = self.items.get(id) else { return };
        let (kind, d, held) = (item.item_type, item.despawn_time, item.parent_human != -1);
        let disk = kind.is_disk();
        let in_ground_gun = (item.parent_item != -1)
            .then(|| self.items.get(item.parent_item as usize))
            .flatten()
            .is_some_and(|p| self.item_types[p.item_type as usize].is_gun && p.parent_human == -1);
        let never_despawns = matches!(kind, ItemKind::CashRound | ItemKind::CashWorld | ItemKind::Phone | ItemKind::Rope);
        let refresh = |d: i32| if (d - 1) as u32 <= 65534 { NEVER_DESPAWN } else { d };
        let item = self.items.get_mut(id).unwrap();
        let countdown = if held && item.parent_item == -1 {
            item.despawn_time = if d > 65535 { NEVER_DESPAWN } else { refresh(d) };
            false
        } else if item.parent_item != -1 {
            if in_ground_gun && !disk {
                if d <= 65535 {
                    true
                } else if !held {
                    item.despawn_time = refresh(d);
                    false
                } else {
                    item.despawn_time = GROUND_DESPAWN;
                    true
                }
            } else {
                item.despawn_time = if d > 65535 && held { NEVER_DESPAWN } else { refresh(d) };
                false
            }
        } else if !disk && d <= 65535 && !never_despawns {
            true
        } else {
            item.despawn_time = refresh(d);
            false
        };
        if !countdown {
            return;
        }
        let d = item.despawn_time;
        let next = if d <= GROUND_DESPAWN { d - 1 } else { GROUND_DESPAWN - 1 };
        item.despawn_time = next;
        if !item.children.is_empty() {
            item.despawn_time = GROUND_DESPAWN;
            if modeless {
                item.despawn_time = NEVER_DESPAWN;
            }
        } else if modeless && next > 0 {
            item.despawn_time = NEVER_DESPAWN;
        }
    }

    pub(crate) fn mark_item_for_deletion(&mut self, item_id: usize) {
        if let Some(item) = self.items.get_mut(item_id) {
            item.despawn_time = 0
        }
    }

    /// The use key on an item in hand: a briefcase opens or closes when pressed, a burger is eaten when let go, and a
    /// bandage patches up the nearest bleeding or dying human while held.
    fn use_logic(&mut self, id: usize) {
        let Some(item) = self.items.get(id) else { return };
        let (input, last) = (item.input_flags, item.last_input_flags);
        let pressed = input & ITEM_USE != 0;
        let kind = item.item_type;
        if !pressed {
            if kind == ItemKind::Burger && last & ITEM_USE != 0 {
                self.burger_logic(id);
            } else if let Some(ItemState::Bandage { progress, .. }) = self.items.get_mut(id).map(|i| &mut i.state) {
                *progress = 0;
            }
            return;
        }
        if last & ITEM_USE == 0 {
            let toggled = match kind {
                ItemKind::Briefcase => Some(ItemKind::BriefcaseOpen),
                ItemKind::BriefcaseOpen => Some(ItemKind::Briefcase),
                _ => None,
            };
            if let Some(t) = toggled {
                self.items.get_mut(id).unwrap().item_type = t;
                return;
            }
        }
        if kind == ItemKind::Bandage {
            self.bandage_logic(id);
        }
    }

    /// The item action (type 2) a player sends for an item in their hands: a computer takes the key, a dialling
    /// phone a digit.
    pub(crate) fn item_action(&mut self, pid: PlayerId, item_id: usize, key: i32) {
        let Some(human) = self.players.get(pid.idx()).and_then(|p| p.human) else { return };
        let Some(h) = self.humans.get(human) else { return };
        let Some(hand) = (0..2).find(|&s| h.inventory[s].count > 0 && h.inventory[s].items[0] == item_id as i32) else { return };

        if self.items.get(item_id).is_some_and(|i| i.item_type == ItemKind::Computer) {
            self.computer_keypress(item_id, key);
        }

        if self.items.get(item_id).is_some_and(|i| i.item_type == ItemKind::CashWorld) {
            self.cash_action(human, hand, item_id, key);
        }

        if !self.is_phone(item_id) {
            return;
        }

        let p = self.items.get_mut(item_id).unwrap().state.phone_mut().unwrap();
        if p.status != PhoneStatus::Dialing || p.cooldown != 0 || !(0..=9).contains(&key) {
            return;
        }

        p.entered_number = key + p.entered_number * 10;
        p.display_number = p.entered_number;

        self.phone_sound(Sound::PHONE_KEYS[key as usize], item_id, 1.0);
        self.phone_update(item_id);
    }
}
