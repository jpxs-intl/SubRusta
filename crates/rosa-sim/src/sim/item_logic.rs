use glam::Vec3;
use rosa_math::vector::Vector;
use rosa_protocol::{
    GameMode,
    clientbound::game::{
        GameState, ItemKind,
        events::{Event, ServerEvent, bullet::EventBullet, explosion::EventExplosion, phone_sound::EventPhoneSound, sound::Sound, update_phone::EventUpdatePhone},
    },
};

use super::{
    Sim,
    bullets::{BULLET_DATA, create_bullet},
    item_state::{Cash, ItemState, PhoneStatus},
};
use crate::human::arms::calculate_spread_vector;
use crate::PlayerId;

const NEVER_DESPAWN: i32 = 0xffff;
const GROUND_DESPAWN: i32 = 18000;
const USE: u32 = 1;
const SECONDARY: u32 = 2;
const PHONE_BUSY_TICKS: i32 = 359;
const PHONE_RING_TICKS: i32 = 767;
const PHONE_DIAL_COOLDOWN: i32 = 180;
const GRENADE_KILL_RANGE: f32 = 4.0;
const GRENADE_PUSH_RANGE: f32 = 8.0;
/// The damage a grenade kill scores for the primer's criminal rating and team kill punishment.
const GRENADE_SCORE: i32 = 100;
const BANDAGE_RANGE: f32 = 2.0;
const BANDAGE_TICKS: i32 = 255;

/// What the use key did on a phone, decided before any other phone is touched.
enum PhoneCall {
    Answer,
    HangUp(usize),
}

impl Sim {
    /// The key part of logic_item: a key keeps while its vehicle stands, and locks it unless whoever holds the key
    /// sits in it, who then owns it.
    fn key_logic(&mut self, id: usize) {
        let Some(item) = self.items.get(id) else { return };
        let ItemState::Key { vehicle: Some(vid) } = item.state else { return };
        let holder = item.parent_human;
        let active = self.vehicles.get(vid).is_some();
        if active && let Some(i) = self.items.get_mut(id) {
            i.despawn_time = 0xffff;
        }
        let Some(v) = self.vehicles.get_mut(vid) else { return };
        v.locked = true;
        if let Some(h) = usize::try_from(holder).ok().and_then(|h| self.humans.get(h))
            && h.vehicle == Some(vid)
        {
            v.locked = false;
            v.owner = h.player.map_or(-1, |p| p.0 as i32);
        }
    }

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
                    let pressed = self.items.get(id).is_some_and(|i| i.input_flags & USE != 0);
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
        if input & USE != 0 {
            if kind == ItemKind::Magnum || *trigger_ticks <= 1 || semi {
                *trigger_ticks += 1;
            }
        } else {
            *trigger_ticks = 0;
            if semi {
                *cooldown = 0;
            }
        }
        let chamber = |items: &mut rosa_physics::Table<super::items::Item>| {
            let Some(m) = mag.and_then(|m| items.get_mut(m)) else { return false };
            if let ItemState::Stock { left } = &mut m.state
                && *left > 0
            {
                *left -= 1;
                return true;
            }
            false
        };

        if *rounds == 0 && chamber(&mut self.items) && let Some(ItemState::Gun { rounds, .. }) = self.items.get_mut(id).map(|i| &mut i.state) {
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
        let mut vel = Vec3::new(r2.x * k + bv.x, r2.y * k + bv.y, k * r2.z + bv.z);
        let s = calculate_spread_vector(&mut self.noise_seed, spread, 0.0);
        vel = Vec3::new(vel.x + s.x, vel.y + s.y, vel.z + s.z);
        let muzzle = Vec3::new(r2.x * 0.0 + p.x, r2.y * 0.0 + p.y, 0.0 * r2.z + p.z);
        let shooter = self.humans.get(holder as usize).and_then(|h| h.player);
        if create_bullet(&mut self.bullets, bullet_type, muzzle, vel, shooter, self.bodies.gravity_scale) {
            self.stats.bullets += 1;
        }
        let shown = Vec3::new(p.x - bv.x, p.y - bv.y, p.z - bv.z);
        let e = EventBullet { bullet_type, item_id: id as i32, pos: Vector(shown), vel: Vector(vel) };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::Bullet(e) });
        let s = calculate_spread_vector(&mut self.noise_seed, spread + spread, 0.0);
        let r = Vec3::new(vel.x + s.x, s.y + vel.y, vel.z + s.z);
        let bm = BULLET_DATA.get(bullet_type as usize).map_or(0.0, |d| d.0);
        let push = -((bm + bm) / gun_mass);
        let lever = Vec3::new(r2.x * LEVER_BACK + r1.x * LEVER_UP, r2.y * LEVER_BACK + r1.y * LEVER_UP, LEVER_UP * r1.z + LEVER_BACK * r2.z);
        let turn = Vec3::new(lever.y * r.z - lever.z * r.y, lever.z * r.x - r.z * lever.x, lever.x * r.y - lever.y * r.x);
        if let Some(body) = self.bodies.get_mut(body_id) {
            body.vel = Vec3::new(push * r.x + body.vel.x, push * r.y + body.vel.y, push * r.z + body.vel.z);
            let l = body.ang_momentum;
            body.ang_momentum = Vec3::new(turn.x * push + l.x, turn.y * push + l.y, push * turn.z + l.z);
        }
        *rounds -= 1;
        *cooldown = fire_rate;
        if *rounds == 1 && chamber(&mut self.items) && let Some(ItemState::Gun { rounds, .. }) = self.items.get_mut(id).map(|i| &mut i.state) {
            *rounds = 1;
        }
    }

    /// Items held, or left inside a gun on the ground, never despawn; anything else lying around despawns after
    /// 18000 ticks (counted twice a tick with cleanup_items), except in round and eliminator.
    fn item_despawn_rules(&mut self, id: usize) {
        let modeless = matches!(self.gamemode, GameMode::Round | GameMode::Eliminator);
        let Some(item) = self.items.get(id) else { return };
        let (kind, d, held) = (item.item_type, item.despawn_time, item.parent_human != -1);
        let disk = kind.is_disk();
        let in_ground_gun = (item.parent_item != -1).then(|| self.items.get(item.parent_item as usize)).flatten().is_some_and(|p| self.item_types[p.item_type as usize].is_gun && p.parent_human == -1);
        let never_despawns = matches!(kind, ItemKind::CashRound | ItemKind::CashWorld | ItemKind::Phone | ItemKind::Rope);
        let refresh = |d: i32| if (d - 1) as u32 <= 0xfffe { NEVER_DESPAWN } else { d };
        let item = self.items.get_mut(id).unwrap();
        let countdown = if held && item.parent_item == -1 {
            item.despawn_time = if d > 0xffff { NEVER_DESPAWN } else { refresh(d) };
            false
        } else if item.parent_item != -1 {
            if in_ground_gun && !disk {
                if d <= 0xffff {
                    true
                } else if !held {
                    item.despawn_time = refresh(d);
                    false
                } else {
                    item.despawn_time = GROUND_DESPAWN;
                    true
                }
            } else {
                item.despawn_time = if d > 0xffff && held { NEVER_DESPAWN } else { refresh(d) };
                false
            }
        } else if !disk && d <= 0xffff && !never_despawns {
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

    pub fn mark_item_for_deletion(&mut self, item_id: usize) {
        if let Some(item) = self.items.get_mut(item_id) { item.despawn_time = 0 }
    }

    /// The use key on an item in hand: a briefcase opens or closes when pressed, a burger is eaten when let go, and a
    /// bandage patches up the nearest bleeding or dying human while held.
    fn use_logic(&mut self, id: usize) {
        let Some(item) = self.items.get(id) else { return };
        let (input, last) = (item.input_flags, item.last_input_flags);
        let pressed = input & USE != 0;
        let kind = item.item_type;
        if !pressed {
            if kind == ItemKind::Burger && last & USE != 0 {
                self.eat_burger(id);
            } else if let Some(ItemState::Bandage { progress, .. }) = self.items.get_mut(id).map(|i| &mut i.state) {
                *progress = 0;
            }
            return;
        }
        if last & USE == 0 {
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
            self.bandage(id);
        }
    }

    fn eat_burger(&mut self, id: usize) {
        let item = self.items.get(id).unwrap();

        let Some(h) = (item.parent_human != -1).then(|| self.humans.get_mut(item.parent_human as usize)).flatten() else { return };

        if h.eat_cooldown != 0 {
            return;
        }

        h.eat_cooldown = 300;
        h.max_stamina = (h.max_stamina + 16).min(255);

        let item = self.items.get_mut(id).unwrap();
        if let ItemState::Burger { bites_left: left } = &mut item.state {
            *left -= 1;

            if *left <= 0 {
                self.mark_item_for_deletion(id);
            }
        }
    }

    fn bandage(&mut self, id: usize) {
        let item = self.items.get_mut(id).unwrap();

        let (pos, holder) = (self.bodies.get(item.body).map_or(item.pos2, |b| b.pos), item.parent_human);

        let mut nearest = (BANDAGE_RANGE, None);

        for (k, h) in self.humans.iter() {
            if !(h.old_health > 0 && (h.bleeding || h.old_health <= 9)) {
                continue;
            }

            let d = Vec3::new(h.pos.x - pos.x, h.pos.y - pos.y, h.pos.z - pos.z);
            let dist = (d.z * d.z + (d.x * d.x + d.y * d.y)).sqrt();

            if nearest.0 > dist {
                nearest = (dist, Some(k));
            }
        }

        let Some(target) = nearest.1 else { return };

        let ItemState::Bandage { usage_left: left, progress } = &mut item.state else { return };
        *progress += 1;

        let p = *progress;
        if holder != -1 && let Some(h) = self.humans.get_mut(holder as usize)
        {
            h.progress_bar = p;
        }

        if p <= BANDAGE_TICKS {
            return;
        }

        *progress = 0;
        *left -= 1;

        if *left <= 0 {
            self.mark_item_for_deletion(id);
        }

        if let Some(t) = self.humans.get_mut(target) {
            t.bleeding = false;

            if t.old_health <= 9 {
                t.old_health = 10;
            }
        }
    }

    /// A grenade: the secondary key pulls the pin (or puts it back while still held); thrown, it counts down from
    /// 239 and blows up.
    fn grenade_logic(&mut self, id: usize) {
        let item = self.items.get_mut(id).unwrap();
        let (input, last, holder) = (item.input_flags, item.last_input_flags, item.parent_human);

        let rising = input & SECONDARY != 0 && last & SECONDARY == 0;

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

    /// grenade_explosion: kills every human within 4 of it and throws the bones of everyone within 8 outwards.
    fn grenade_explosion(&mut self, at: Vec3, primer: Option<PlayerId>) {
        let mut killed = Vec::new();
        for (_, h) in self.humans.iter_mut() {
            let p = h.bones[0].pos;
            let (dx, dy, dz) = (p.x - at.x, p.y - at.y, p.z - at.z);
            let dist = (dz * dz + (dx * dx + dy * dy)).sqrt();
            if !(GRENADE_KILL_RANGE <= dist) {
                h.old_health = 0;
                killed.push(h.player);
            }
            if GRENADE_PUSH_RANGE <= dist {
                continue;
            }
            for bone in &h.bones {
                let Some(b) = self.bodies.get_mut(bone.body) else { continue };
                let d = Vec3::new(b.pos.x - at.x, b.pos.y - at.y, b.pos.z - at.z);
                let len = ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt();
                let dir = if len != 0.0 {
                    let inv = 1.0 / len;
                    Vec3::new(d.x * inv, d.y * inv, d.z * inv)
                } else {
                    Vec3::ZERO
                };
                let weight = (GRENADE_PUSH_RANGE / (bone.mass + GRENADE_PUSH_RANGE)).max(0.125);
                let v = (GRENADE_PUSH_RANGE - len) * 0.09375;
                let v = ((v * v) * 1.5) * weight;
                b.vel = Vec3::new(dir.x * v + b.vel.x, dir.y * v + b.vel.y, v * dir.z + b.vel.z);
            }
        }
        let Some(primer) = primer else { return };
        for victim in killed.into_iter().flatten().filter(|&v| v != primer) {
            if self.same_team(primer, victim) {
                self.punish_team_kill(primer, GRENADE_SCORE);
            }
            self.handle_criminal_rating(primer, victim, GRENADE_SCORE);
        }
    }

    pub(super) fn phone_update(&mut self, id: usize) {
        let Some(p) = self.items.get(id).and_then(|i| i.state.phone()) else { return };
        let e = EventUpdatePhone { item_id: id as i32, phone_status: p.status as i32, display_phone_number: p.display_number, phone_texture: p.texture };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::UpdatePhone(e) });
    }

    /// create_event_phone: a sound the item plays (event 0x13).
    pub(crate) fn phone_sound(&mut self, sound: Sound, id: usize, volume: f32) {
        let e = EventPhoneSound { sound, item_id: id as i32, volume, pitch: 1.0 };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::PhoneSound(e) });
    }

    fn is_phone(&self, id: usize) -> bool {
        self.items.get(id).is_some_and(|i| matches!(i.item_type, ItemKind::Phone | ItemKind::PhonePay))
    }

    /// The phone part of logic_item: picking up, dialling a four digit number, ringing, answering and hanging up.
    fn phone_logic(&mut self, id: usize) {
        let tick = self.tick;
        let item = self.items.get_mut(id).unwrap();
        let rising = item.input_flags & USE != 0 && item.last_input_flags & USE == 0;
        let Some(p) = item.state.phone_mut() else { return };
        if p.cooldown > 0 {
            p.cooldown -= 1;
        }
        match p.status {
            PhoneStatus::Idle => {
                p.ring_timer = 0;
                if rising {
                    p.status = PhoneStatus::Dialing;
                    p.entered_number = 0;
                    p.display_number = 0;
                    self.phone_update(id);
                }
                return;
            }
            PhoneStatus::Busy => {
                p.ring_timer += 1;
                let timer = p.ring_timer;
                if tick & 0x3f == 0 {
                    self.phone_sound(Sound::PhoneBusy, id, 1.0);
                }
                if timer > PHONE_BUSY_TICKS || rising {
                    let p = self.items.get_mut(id).unwrap().state.phone_mut().unwrap();
                    p.status = PhoneStatus::Idle;
                    p.ring_timer = 0;
                    p.connected = None;
                    p.display_number = 0;
                    p.entered_number = 0;
                    self.phone_update(id);
                }
                return;
            }
            PhoneStatus::Dialing => {
                p.ring_timer = 0;
                if rising {
                    p.status = PhoneStatus::Idle;
                    p.entered_number = 0;
                    p.display_number = 0;
                    self.phone_update(id);
                }
                let p = self.items.get_mut(id).unwrap().state.phone_mut().unwrap();
                let number = p.entered_number;
                if number <= 999 {
                    return;
                }
                p.entered_number = 0;
                p.cooldown = PHONE_DIAL_COOLDOWN;
                return self.place_call(id, number);
            }
            PhoneStatus::Connected => {
                let other = p.connected;
                let gone = match other {
                    None => true,
                    Some(o) => self.items.get(o).is_none(),
                };
                if gone {
                    let p = self.items.get_mut(id).unwrap().state.phone_mut().unwrap();
                    p.status = PhoneStatus::Idle;
                    if other.is_some() {
                        p.connected = None;
                    }
                    p.display_number = 0;
                    self.phone_update(id);
                }
            }
            PhoneStatus::Ringing => {}
        }
        let p = self.items.get(id).unwrap().state.phone().unwrap();
        if p.status == PhoneStatus::Ringing {
            let other = p.connected;
            let answered = other.is_none_or(|o| self.items.get(o).and_then(|i| i.state.phone()).is_some_and(|q| q.connected.is_some()));
            if answered {
                self.items.get_mut(id).unwrap().state.phone_mut().unwrap().ring_timer = 0;
            } else {
                let o = other.unwrap();
                if tick & 0x7f == 0 {
                    self.phone_sound(Sound::PhoneRing, o, 1.0);
                    self.phone_sound(Sound::PhoneRing, id, 0.5);
                }
                let p = self.items.get_mut(id).unwrap().state.phone_mut().unwrap();
                p.ring_timer += 1;
                if p.ring_timer > PHONE_RING_TICKS {
                    p.status = PhoneStatus::Idle;
                    if let Some(q) = self.items.get_mut(o).and_then(|i| i.state.phone_mut()) {
                        q.status = PhoneStatus::Idle;
                        q.display_number = 0;
                    }
                    self.phone_update(o);
                    let p = self.items.get_mut(id).unwrap().state.phone_mut().unwrap();
                    p.connected = None;
                    p.display_number = 0;
                    self.phone_update(id);
                }
            }
        }
        if !rising {
            return;
        }
        let call = match self.items.get(id).unwrap().state.phone().unwrap().connected {
            None => PhoneCall::Answer,
            Some(o) => PhoneCall::HangUp(o),
        };
        match call {
            PhoneCall::Answer => self.answer(id),
            PhoneCall::HangUp(o) => {
                self.items.get_mut(id).unwrap().state.phone_mut().unwrap().status = PhoneStatus::Idle;
                if let Some(q) = self.items.get_mut(o).and_then(|i| i.state.phone_mut()) {
                    q.status = PhoneStatus::Idle;
                    q.connected = None;
                    q.display_number = 0;
                }
                self.phone_update(o);
                let p = self.items.get_mut(id).unwrap().state.phone_mut().unwrap();
                p.connected = None;
                p.display_number = 0;
                self.phone_update(id);
            }
        }
    }

    /// A fully dialled number rings the phone with that number, or gets the busy tone if it is already in a call.
    fn place_call(&mut self, id: usize, number: i32) {
        let target = self.items.ids().into_iter().find(|&k| k != id && self.is_phone(k) && self.items.get(k).and_then(|i| i.state.phone()).is_some_and(|q| q.number == number));
        let Some(o) = target else { return };
        let busy = self.items.get(o).unwrap().state.phone().unwrap().connected.is_some();
        let own_number = self.items.get(id).unwrap().state.phone().unwrap().number;
        let p = self.items.get_mut(id).unwrap().state.phone_mut().unwrap();
        if busy {
            p.status = PhoneStatus::Busy;
            p.display_number = 0;
            self.phone_update(id);
            return;
        }
        p.status = PhoneStatus::Ringing;
        p.connected = Some(o);
        p.display_number = 0;
        let q = self.items.get_mut(o).unwrap().state.phone_mut().unwrap();
        q.status = PhoneStatus::Ringing;
        q.display_number = own_number;
        self.phone_update(id);
        self.phone_update(o);
    }

    /// Picking up a ringing phone connects it to every phone calling it.
    fn answer(&mut self, id: usize) {
        let own_number = self.items.get(id).unwrap().state.phone().unwrap().number;
        for k in self.items.ids() {
            if k == id || !self.is_phone(k) || self.items.get(k).unwrap().state.phone().unwrap().connected != Some(id) {
                continue;
            }
            let caller_number = self.items.get(k).unwrap().state.phone().unwrap().number;
            let p = self.items.get_mut(id).unwrap().state.phone_mut().unwrap();
            p.status = PhoneStatus::Connected;
            p.connected = Some(k);
            p.display_number = caller_number;
            let q = self.items.get_mut(k).unwrap().state.phone_mut().unwrap();
            q.status = PhoneStatus::Connected;
            q.display_number = own_number;
            self.phone_update(id);
            self.phone_update(k);
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

    /// The world cash part of logic_playerinteractions: key 0 and 1 move the pick along the stack; a higher key (with
    /// enough bills) hands the picked bill to the other hand, onto its stack or as a stack of its own.
    fn cash_action(&mut self, human: usize, hand: usize, item_id: usize, key: i32) {
        let Some(cash) = self.items.get(item_id).and_then(|i| i.state.cash()).copied() else { return };
        match key {
            0 => self.items.get_mut(item_id).unwrap().state.cash_mut().unwrap().spread += 1,
            1 => self.items.get_mut(item_id).unwrap().state.cash_mut().unwrap().spread -= 1,
            _ if cash.bills >= key - 2 => self.move_bill(human, hand, item_id, cash),
            _ => {}
        }
        let Some(c) = self.items.get_mut(item_id).and_then(|i| i.state.cash_mut()) else { return };
        if c.spread < 0 {
            c.spread = 0;
        }
        if c.bills < c.spread {
            c.spread = c.bills;
        }
    }

    /// The picked bill to the other hand: onto the stack there, or as a new one-bill stack when that hand is empty.
    fn move_bill(&mut self, human: usize, hand: usize, item_id: usize, cash: Cash) {
        let other = hand ^ 1;
        let code = cash.code(cash.spread);
        let held = self.humans.get(human).and_then(|h| (h.inventory[other].count > 0).then_some(h.inventory[other].items[0] as usize));
        match held {
            None => {
                let Some(item) = self.items.get(item_id) else { return };
                let (vel, body) = (item.vel, item.body);
                let Some((pos, rot)) = self.bodies.get(body).map(|b| (b.pos, b.rot)) else { return };
                if let Some(new) = self.create_item(ItemKind::CashWorld, pos, Some(vel), rot) {
                    if let Some(c) = self.items.get_mut(new).and_then(|i| i.state.cash_mut()) {
                        c.bills = 0;
                        c.codes = code;
                    }
                    let Sim { humans, bodies, item_grid, items, item_types, vehicles, vehicle_types, .. } = self;
                    if let Some(h) = humans.get_mut(human) {
                        let mut touch = super::items::Touchables { grid: item_grid, items, types: item_types, vehicles, vehicle_types, occupied: Vec::new() };
                        crate::human::inventory::link_item_to_human(h, human, bodies, &mut touch, new, other);
                    }
                }
            }
            Some(o) => {
                let added = self.items.get_mut(o).and_then(|i| i.state.cash_mut()).is_some_and(|c| c.insert(0, code));
                if !added {
                    return;
                }
            }
        }
        let Some(item) = self.items.get_mut(item_id) else { return };
        let Some(c) = item.state.cash_mut() else { return };
        let spread = c.spread;
        if !c.remove(spread) {
            item.despawn_time = 0;
        }
    }
}
