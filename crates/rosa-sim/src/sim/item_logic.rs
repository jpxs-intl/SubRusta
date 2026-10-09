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
    item_state::{ItemState, PhoneStatus},
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
const GRENADE_HELD_FUSE: i32 = 240;
const GRENADE_KILL_RANGE: f32 = 4.0;
const GRENADE_PUSH_RANGE: f32 = 8.0;
const BANDAGE_RANGE: f32 = 2.0;
const BANDAGE_TICKS: i32 = 255;
const BURGER_COOLDOWN: i32 = 300;

/// What the use key did on a phone, decided before any other phone is touched.
enum PhoneCall {
    Answer,
    HangUp(usize),
}

impl Sim {
    /// The per-item part of logic_item: despawn timers, then each type's behaviour, then the item's keys move to
    /// last tick's.
    pub(crate) fn item_behaviours(&mut self) {
        // TODO: the walkie-talkie channel lists (read by calculate_voice) and items placed in other items
        for id in self.items.ids() {
            self.item_despawn_rules(id);
            let Some(kind) = self.items.get(id).map(|i| i.item_type) else { continue };
            if self.item_types[kind as usize].is_gun {
                self.fire_gun(id);
            }
            // TODO: a key locking its vehicle, logic_computer
            match kind {
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
        if *rounds == 0 && chamber(&mut self.items) {
            if let Some(ItemState::Gun { rounds, .. }) = self.items.get_mut(id).map(|i| &mut i.state) {
                *rounds = 1;
            }
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
        create_bullet(&mut self.bullets, bullet_type, muzzle, vel, shooter);
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
        if *rounds == 1 && chamber(&mut self.items) {
            if let Some(ItemState::Gun { rounds, .. }) = self.items.get_mut(id).map(|i| &mut i.state) {
                *rounds = 1;
            }
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
        // TODO: an item holding other items stays at 18000
        if modeless && next > 0 {
            item.despawn_time = NEVER_DESPAWN;
        }
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
                self.eat(id);
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

    fn eat(&mut self, id: usize) {
        let item = self.items.get(id).unwrap();
        let Some(h) = (item.parent_human != -1).then(|| self.humans.get_mut(item.parent_human as usize)).flatten() else { return };
        if h.unk_40 != 0 {
            return;
        }
        h.unk_40 = BURGER_COOLDOWN;
        h.max_stamina = (h.max_stamina + 16).min(255);
        h.unk_3c = (h.unk_3c + 8).min(105);
        let item = self.items.get_mut(id).unwrap();
        if let ItemState::Burger { left } = &mut item.state {
            *left -= 1;
            if *left <= 0 {
                item.despawn_time = 0;
            }
        }
    }

    fn bandage(&mut self, id: usize) {
        let item = self.items.get(id).unwrap();
        let (pos, holder) = (self.bodies.get(item.body).map_or(item.pos2, |b| b.pos), item.parent_human);
        let mut nearest = (BANDAGE_RANGE, None);
        for (k, h) in self.humans.iter() {
            if !(h.old_health > 0 && (h.unk_6d80 != 0 || h.old_health <= 9)) {
                continue;
            }
            let d = Vec3::new(h.pos.x - pos.x, h.pos.y - pos.y, h.pos.z - pos.z);
            let dist = (d.z * d.z + (d.x * d.x + d.y * d.y)).sqrt();
            if nearest.0 > dist {
                nearest = (dist, Some(k));
            }
        }
        let Some(target) = nearest.1 else { return };
        let item = self.items.get_mut(id).unwrap();
        let ItemState::Bandage { left, progress } = &mut item.state else { return };
        *progress += 1;
        let p = *progress;
        if holder != -1
            && let Some(h) = self.humans.get_mut(holder as usize)
        {
            h.progress_bar = p;
        }
        if p <= BANDAGE_TICKS {
            return;
        }
        *progress = 0;
        *left -= 1;
        if *left <= 0 {
            item.despawn_time = 0;
        }
        if let Some(t) = self.humans.get_mut(target) {
            t.unk_6d80 = 0;
            if t.old_health <= 9 {
                t.old_health = 10;
            }
        }
    }

    /// A grenade: the secondary key pulls the pin (or puts it back while still held); thrown, it counts down from
    /// 239 and blows up.
    fn grenade_logic(&mut self, id: usize) {
        let item = self.items.get(id).unwrap();
        let (input, last, holder) = (item.input_flags, item.last_input_flags, item.parent_human);
        let rising = input & SECONDARY != 0 && last & SECONDARY == 0;
        let holder_player = (holder != -1).then(|| self.humans.get(holder as usize)).flatten().and_then(|h| h.player);
        let item = self.items.get_mut(id).unwrap();
        let ItemState::Grenade { pin, fuse, primer } = &mut item.state else { return };
        if *pin > 0 {
            if rising {
                *fuse = GRENADE_HELD_FUSE;
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
        if c == GRENADE_HELD_FUSE {
            if rising {
                *pin = 1;
                *primer = None;
            } else if holder == -1 {
                *fuse = GRENADE_HELD_FUSE - 1;
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
        self.grenade_explosion(pos);
    }

    /// grenade_explosion: kills every human within 4 of it and throws the bones of everyone within 8 outwards.
    fn grenade_explosion(&mut self, at: Vec3) {
        // TODO: the primer's team kill punishment and criminal rating
        for (_, h) in self.humans.iter_mut() {
            let p = h.bones[0].pos;
            let (dx, dy, dz) = (p.x - at.x, p.y - at.y, p.z - at.z);
            let dist = (dz * dz + (dx * dx + dy * dy)).sqrt();
            if !(GRENADE_KILL_RANGE <= dist) {
                h.old_health = 0;
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
    }

    fn phone_update(&mut self, id: usize) {
        let Some(p) = self.items.get(id).and_then(|i| i.state.phone()) else { return };
        let e = EventUpdatePhone { item_id: id as i32, phone_status: p.status as i32, display_phone_number: p.display_number, phone_texture: p.texture };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::UpdatePhone(e) });
    }

    fn phone_sound(&mut self, sound: Sound, id: usize, volume: f32) {
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

    /// The item action (type 2) a player sends for an item in their hands: a dialling phone takes a digit.
    pub(crate) fn item_action(&mut self, pid: PlayerId, item_id: usize, key: i32) {
        // TODO: computer key presses (computer_handle_keypress) and splitting world cash
        let Some(h) = self.players.get(pid.idx()).and_then(|p| p.human).and_then(|id| self.humans.get(id)) else { return };
        let in_hand = (0..2).any(|s| h.inventory[s].count > 0 && h.inventory[s].items[0] == item_id as i32);
        if !in_hand || !self.is_phone(item_id) {
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
