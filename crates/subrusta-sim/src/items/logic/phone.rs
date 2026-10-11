use subrusta_protocol::clientbound::game::{
    ItemKind,
    events::{Event, ServerEvent, phone_sound::EventPhoneSound, sound::Sound, update_phone::EventUpdatePhone},
};

use crate::{input::ITEM_USE, sim::Sim};

const PHONE_BUSY_TICKS: i32 = 359;
const PHONE_RING_TICKS: i32 = 767;
const PHONE_DIAL_COOLDOWN: i32 = 180;

/// What the use key did on a phone, decided before any other phone is touched.
enum PhoneCall {
    Answer,
    HangUp(usize),
}

/// Where a phone is in a call (item +0x27c).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum PhoneStatus {
    #[default]
    Idle = 0,
    Dialing = 1,
    /// Calling out, or being called while not yet answered.
    Ringing = 2,
    Connected = 3,
    Busy = 4,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Phone {
    pub number: i32,
    pub status: PhoneStatus,
    pub connected: Option<usize>,
    /// Ticks spent ringing or busy (+0x164).
    pub ring_timer: i32,
    /// The number on the screen (+0x168).
    pub display_number: i32,
    /// The digits dialled so far (+0x16c).
    pub entered_number: i32,
    /// Ticks before the keypad works again after a call is placed (+0x13c).
    pub cooldown: i32,
    /// +0x278
    pub texture: i32,
    /// Where a pay phone was put (+0x300).
    pub home: glam::Vec3,
}

impl Sim {
    pub(crate) fn phone_update(&mut self, id: usize) {
        let Some(p) = self.items.get(id).and_then(|i| i.state.phone()) else { return };
        let e = EventUpdatePhone { item_id: id as i32, phone_status: p.status as i32, display_phone_number: p.display_number, phone_texture: p.texture };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::UpdatePhone(e) });
    }

    /// create_event_phone: a sound the item plays (event 0x13).
    pub(crate) fn phone_sound(&mut self, sound: Sound, id: usize, volume: f32) {
        let e = EventPhoneSound { sound, item_id: id as i32, volume, pitch: 1.0 };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::PhoneSound(e) });
    }

    pub(crate) fn is_phone(&self, id: usize) -> bool {
        self.items.get(id).is_some_and(|i| matches!(i.item_type, ItemKind::Phone | ItemKind::PhonePay))
    }

    /// The phone part of logic_item: picking up, dialling a four digit number, ringing, answering and hanging up.
    pub(crate) fn phone_logic(&mut self, id: usize) {
        let tick = self.tick;
        let item = self.items.get_mut(id).unwrap();
        let rising = item.input_flags & ITEM_USE != 0 && item.last_input_flags & ITEM_USE == 0;

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

        let phone = self.items.get(id).unwrap().state.phone().unwrap();
        if phone.status == PhoneStatus::Ringing {
            let other = phone.connected;
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

                    let phone_state = self.items.get_mut(id).unwrap().state.phone_mut().unwrap();
                    phone_state.connected = None;
                    phone_state.display_number = 0;

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
        let target = self
            .items
            .ids()
            .into_iter()
            .find(|&k| k != id && self.is_phone(k) && self.items.get(k).and_then(|i| i.state.phone()).is_some_and(|q| q.number == number));
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
}
