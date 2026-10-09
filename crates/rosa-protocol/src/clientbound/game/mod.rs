use rosa_math::vector::Vector;
use serde::{Deserialize, Serialize};

use crate::{CharacterCustomization, clientbound::game::events::Event, codec::{WireWrite, Writer}, serverbound::game::voice::VoiceFrame};

pub mod events;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Auto5 = 0,
    Ak47 = 1,
    Ak47Mag = 2,
    M16 = 3,
    M16Mag = 4,
    Magnum = 5,
    MagnumMag = 6,
    Mp5 = 7,
    Mp5Mag = 8,
    Uzi = 9,
    UziMag = 10,
    Pistol = 11,
    PistolMag = 12,
    Grenade = 13,
    Bandage = 14,
    Briefcase = 15,
    BriefcaseOpen = 16,
    CashRound = 17,
    CashWorld = 18,
    DiskBlack = 19,
    DiskGreen = 20,
    DiskBlue = 21,
    DiskWhite = 22,
    DiskGold = 23,
    DiskRed = 24,
    Phone = 25,
    Radio = 26,
    Key = 27,
    Door = 28,
    PaperWorld = 29,
    Burger = 30,
    Desk = 31,
    Lamp = 32,
    PhonePay = 33,
    Paper = 34,
    SoccerBall = 35,
    Rope = 36,
    Box = 37,
    BigBox = 38,
    Computer = 39,
    Arcade = 40,
    Table = 41,
    TableTest = 42,
    Wall = 43,
    Bottle = 44,
    Watermelon = 45,
}

impl ItemKind {
    pub const COUNT: usize = 46;

    pub fn is_weapon(self) -> bool {
        matches!(self, Self::Auto5 | Self::Ak47 | Self::M16 | Self::Magnum | Self::Mp5 | Self::Uzi | Self::Pistol | Self::Grenade)
    }

    pub fn is_magazine(self) -> bool {
        matches!(self, Self::Ak47Mag | Self::M16Mag | Self::MagnumMag | Self::Mp5Mag | Self::UziMag | Self::PistolMag)
    }

    pub fn is_disk(self) -> bool {
        (Self::DiskBlack as u8..=Self::DiskRed as u8).contains(&(self as u8))
    }
}

impl TryFrom<u8> for ItemKind {
    type Error = u8;

    fn try_from(v: u8) -> Result<Self, u8> {
        if (v as usize) < Self::COUNT {
            Ok(unsafe { std::mem::transmute::<u8, ItemKind>(v) })
        } else {
            Err(v)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuType {
    Empty = 0,
    EnterCity = 1,
    Lobby = 2,
    EmptyBase = 3,
    WorldCarShop = 9,
    WorldStore = 10,
    WorldStoreDone = 11,
    WorldBank = 12,
    WorldBank2 = 13,
    RoundCorpWeapons = 14,
    RoundCorpAmmo = 15,
    RoundCorpEquip = 16,
    RoundCorpVehicle = 17,
    RoundCorpStock = 18,
    WorldEmptyCorp = 19,
    WorldCorpApplication = 20,
    WorldCorpHiring = 22,
    WorldCorpFiring = 23,
    WorldCorpTeam = 24,
    WorldCorpRequistion = 25,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum GameState {
    Idle = 0,
    #[default]
    Intermission = 1,
    InGame = 2,
    Restarting = 3,
    Paused = 4
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerVoiceData {
    pub player_id: i32,
    pub human_id: i32,
    pub item_id: i32,
    pub voice_frames: [VoiceFrame; 4]
}

/// One entry of a client's object slot ring: a slot taken by an object (pack) or given back (unpack).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectPack {
    pub slot: u16,
    pub unpack: bool,
    /// 0 human, 1 item.
    pub kind: u8,
    pub item_type: u16,
    pub index: u16,
}

impl ObjectPack {
    fn write(&self, w: &mut Writer) {
        w.bits(self.slot as i32, 10);
        w.bits(self.unpack as i32, 2);
        w.bits(self.kind as i32, 3);
        w.bits(self.item_type as i32, 10);
        w.bits(self.index as i32, 16);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerItemObject {
    pub slot: u16,
    pub item_id: u16,
    pub item_type: ItemKind,
    pub pos: Vector,
    pub rot: [f32; 4],
    pub parent_item: i32,
    pub parent_human: i32,
    pub parent_slot: i32,
    pub tail: ItemTail,
}

/// The type-specific end of an item's state (write_iteminfo_to_object).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ItemTail {
    #[default]
    None,
    /// World cash: one less than the bill count (4 bits), the picked bill (4) and the bill codes (30).
    Cash { bills: i32, spread: i32, codes: u32 },
    /// A walkie-talkie: whether it is transmitting.
    Radio(bool),
    /// A computer's cursor (12 bits).
    Computer(i32),
}

impl ServerItemObject {
    fn write_body(&self, w: &mut Writer) {
        w.bits(1, 1);
        w.bits(1, 1);
        w.bits(self.item_type as i32, 8);
        w.bits(self.parent_item, 10);
        w.bits(self.parent_human, 10);
        w.bits(self.parent_slot, 4);
        w.bits(0, 8);

        let p = self.pos.0;
        for v in [(p.x + 4096.0) * 4096.0, p.y * 4096.0, (p.z + 4096.0) * 4096.0] {
            write_absolute(w, v as i32, 28);
        }

        write_quaternion(w, self.rot, 14);
        match self.tail {
            ItemTail::None => {}
            ItemTail::Cash { bills, spread, codes } => {
                w.bits(bills, 4);
                w.bits(spread, 4);
                w.bits(codes as i32, 30);
            }
            ItemTail::Radio(on) => w.bits(on as i32, 1),
            ItemTail::Computer(cursor) => w.bits(cursor, 12),
        }
    }
}

/// A vehicle's state in the game packet (the vehicle part of append_object_packet), always written in full.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerVehicleObject {
    pub vehicle_id: u16,
    pub traffic_car: i32,
    pub pos: Vector,
    pub rot: [f32; 4],
    /// The steering angle as a share of half a turn, in 255ths.
    pub steer: i32,
    /// Per wheel: its suspension height (0 to 255), spin (-255 to 255) and skid (0 to 255).
    pub wheels: [[i32; 3]; 4],
    pub engine_rpm: i32,
}

impl ServerVehicleObject {
    fn write_body(&self, w: &mut Writer) {
        // TODO: the update tier (2 bits: the two most overdue vehicles 0, the next six 1, the rest 2) and the
        // per-connection snapshot each delta is taken from
        w.bits(self.vehicle_id as i32, 10);
        w.bits(0, 2);
        w.bits(self.traffic_car, 10);
        w.bits(0, 8);
        let p = self.pos.0;
        for v in [(p.x + 4096.0) * 4096.0, (0.0 + p.y) * 4096.0, (p.z + 4096.0) * 4096.0] {
            write_absolute(w, v as i32, 28);
        }
        write_quaternion(w, self.rot, 14);
        write_absolute(w, self.steer, 9);
        for [height, spin, skid] in self.wheels {
            write_absolute(w, height, 8);
            write_absolute(w, spin, 9);
            write_absolute(w, skid, 8);
        }
        w.bits(self.engine_rpm, 13);
    }
}

/// A traffic car the client is told about this packet (the traffic part of the game packet).
#[derive(Debug, Clone, PartialEq)]
pub struct TrafficEntry {
    pub index: u16,
    /// Whether the car is a real vehicle right now.
    pub vehicle: bool,
    pub kind: i32,
    pub color: i32,
    pub pos: Vector,
    pub yaw: f32,
}

/// The double precision pi the yaw is scaled by.
const TRAFFIC_PI: f64 = 3.14159265359;

impl TrafficEntry {
    fn write_body(&self, w: &mut Writer) {
        // TODO: the tick of the client's snapshot (8 bits) and deltas from it; every value is sent absolute, and the
        // type and colour every time rather than when the car's generation changes
        w.bits(1, 1);
        w.bits(0, 8);
        w.bits(self.vehicle as i32, 1);
        w.bits(1, 1);
        w.bits(self.kind, 6);
        w.bits(self.color, 4);
        let p = self.pos.0;
        for v in [(p.x + 4096.0) * 4096.0, (p.y + 0.0) * 4096.0, (p.z + 4096.0) * 4096.0] {
            write_absolute(w, v as i32, 28);
        }
        let yaw = ((self.yaw as f64 / TRAFFIC_PI * 255.0) as i32).clamp(-255, 255);
        write_absolute(w, yaw, 9);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerHumanObject {
    pub slot: u16,
    pub human_id: u16,
    pub player_id: i32,
    pub customization: CharacterCustomization,
    pub vehicle: i32,
    pub vehicle_seat: i32,
    pub alive: bool,
    pub bleeding: bool,
    pub pos: Vector,
    pub rot: [f32; 4],
    pub bones: [[f32; 4]; 15],
}

impl ServerHumanObject {
    fn write_body(&self, w: &mut Writer) {
        w.bits(1, 1);
        w.bits(1, 1);
        let c = &self.customization;
        w.bits(self.player_id, 8);
        w.bits(c.gender as i32, 1);
        w.bits(c.head as i32, 5);
        w.bits(c.skin as i32, 3);
        w.bits(c.hair_style as i32, 4);
        w.bits(c.eye_color as i32, 3);
        w.bits(c.hair_color as i32, 4);
        w.bits(c.model as i32, 5);
        w.bits(c.suit_color as i32, 4);
        w.bits(c.tie_color as i32, 4);
        w.bits(c.necklace as i32, 4);
        w.bits(0, 2);

        w.bits(0, 8);
        w.bits(self.vehicle, 8);
        w.bits(self.vehicle_seat, 4);
        w.bits(self.alive as i32, 1);
        w.bits(self.bleeding as i32, 1);

        let p = self.pos.0;
        for v in [(4096.0 + p.x) * 4096.0, (0.0 + p.y) * 4096.0, (4096.0 + p.z) * 4096.0] {
            write_absolute(w, v as i32, 28);
        }
        write_quaternion(w, self.rot, 14);
        for bone in self.bones {
            w.bits(1, 1);
            write_quaternion(w, bone, 12);
        }
    }
}

fn write_quaternion(w: &mut Writer, q: [f32; 4], bits: u32) {
    let (largest, rest) = pack_quaternion(q, bits);
    w.bits(largest, 2);
    for v in rest {
        write_absolute(w, v, bits);
    }
}

fn write_absolute(w: &mut Writer, value: i32, bits: u32) {
    w.bits(1, 1);
    w.bits(1, 1);
    w.bits(value, bits);
}

fn pack_quaternion(q: [f32; 4], bits: u32) -> (i32, [i32; 3]) {
    let scale = (1 << (bits - 1)) as f64 - 1.0;
    let mut c = q;
    let mut largest = 0;
    for i in 1..4 {
        if c[i].abs() > c[largest].abs() {
            largest = i;
        }
    }
    if c[largest] < 0.0 {
        c = c.map(|v| -v);
    }
    let mut out = [0i32; 3];
    let mut k = 0;
    for i in [3usize, 1, 2, 0] {
        if i != largest {
            out[k] = (c[i] as f64 * std::f64::consts::FRAC_1_SQRT_2 * scale) as i32;
            k += 1;
        }
    }
    (largest as i32, out)
}

/// The state of the client's own human, sent in every game packet while the player has a human.
#[derive(Debug, Clone, PartialEq)]
pub struct OwnHumanData {
    pub human_id: i32,
    pub view_yaw: f32,
    pub view_pitch: f32,
    pub yaw_offset: f32,
    pub pitch_offset: f32,
    pub body_yaw: f32,
    pub is_standing: bool,
    pub pain: i32,
    pub unk_6e08: i32,
    pub unk_6e10: i32,
    pub unk_6e14: i32,
    pub unk_6e18: i32,
    pub progress_bar: i32,
    pub health: [i32; 7],
    pub stamina: i32,
    pub max_stamina: i32,
    pub head_vel: Vector,
    /// The inventory display: per slot (hands, then pockets) the item types, with ammo and attachment flags.
    pub inventory: [Vec<i32>; 7],
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerGamePacket {
    pub client_id: u32,
    pub received_actions: u32,
    pub round_number: u32,
    pub network_tick: u32,
    pub last_sdl_tick: u32,
    pub menu_type: MenuType,
    /// The building whose menu is open, and in a shop's list (menu 10) what it sells: type, price and a third value.
    pub menu_tab: i32,
    pub shop: Vec<(i32, i32, i32)>,
    pub money: i32,
    pub gamestate: GameState,
    pub ready_states: Option<[bool; 32]>,
    pub voice: [Option<ServerVoiceData>; 8],

    pub follow_pos: Vector,

    pub own_human: Option<OwnHumanData>,
    pub humans: Vec<ServerHumanObject>,
    pub items: Vec<ServerItemObject>,
    pub vehicles: Vec<ServerVehicleObject>,
    /// The pack entries the client has not acknowledged yet, starting at `pack_offset` in its 2048 entry ring.
    pub object_packs: Vec<ObjectPack>,
    pub pack_offset: u16,
    /// How many traffic cars there are, and the ones sent this packet in index order.
    pub traffic_count: i32,
    pub traffic: Vec<TrafficEntry>,
    /// One intersection's index and its first four lights, a different intersection each packet.
    pub signal: (i32, [i32; 4]),

    pub global_event_count: u32,
    pub first_event: u32,
    pub events: Vec<(u32, Event)>,

    // if game_state is Intermission or Restarting
    //pub corporation_money: Option<ClientboundGamePacketCorporationMoney>,
}

impl WireWrite for ServerGamePacket {
    fn write(&self, w: &mut Writer) {
        w.byte(0x05);
        w.u32(self.round_number);
        w.i32(self.network_tick as i32);

        w.bits(self.gamestate as i32, 4);

        if self.gamestate == GameState::Intermission {
            for r in self.ready_states.unwrap() {
                w.bits(r as i32, 1);
            }
        }

        if self.gamestate == GameState::Intermission || self.gamestate == GameState::Restarting {
            for i in 0..3 {
                w.bits(10 * i, 16);
                w.bits(10 * i, 16);
            }
        }

        w.bits(7200, 24);
        w.bits(9, 16);
        w.bits(get_sun_time(12, 60), 30);

        for _ in 0..5 {
            w.bits(0, 6);
        }

        w.bits(self.client_id as i32, 8);
        w.bits(self.own_human.as_ref().map_or(-1, |h| h.human_id), 10);

        let head_vel = match &self.own_human {
            Some(h) => {
                for v in [h.view_yaw, h.view_pitch, h.yaw_offset, h.pitch_offset, h.body_yaw] {
                    w.f32(v);
                }
                w.bits(h.is_standing as i32, 1);
                w.bits(h.pain, 8);
                w.bits(h.unk_6e08, 8);
                w.bits(h.unk_6e10, 7);
                w.bits(h.unk_6e14, 4);
                w.bits(h.unk_6e18, 4);
                w.bits(h.progress_bar, 8);
                for hp in h.health {
                    w.bits(hp, 7);
                }
                w.bits(h.stamina, 8);
                w.bits(h.max_stamina, 8);
                h.head_vel
            }
            None => Vector::new(0.0, 0.0, 0.0),
        };
        w.f32(head_vel.0.x);
        w.f32(head_vel.0.y);
        w.f32(head_vel.0.z);

        w.bits(0, 1);
        w.bits(self.menu_type as i32, 8);
        w.bits(self.menu_tab, 16);
        if self.menu_type == MenuType::WorldStore {
            w.bits(self.shop.len() as i32, 8);
            for &(kind, price, extra) in &self.shop {
                w.bits(kind, 8);
                w.bits(price, 24);
                w.bits(extra, 4);
            }
        }

        w.i32(self.money);
        w.u32(0);
        w.u32(0);
        w.u32(0);
        w.u32(24);

        w.bits(0, 16);
        w.bits(self.received_actions as i32, 8);

        w.bits(0, 10);
        w.bits(0, 6);
        w.bits(0, 8);

        for k in 0..7 {
            let slot = self.own_human.as_ref().map_or(&[][..], |h| &h.inventory[k][..]);
            w.bits(slot.len() as i32, 4);
            for &t in slot {
                w.bits(t, 16);
            }
        }

        w.bits(0, 8);
        w.bits(0, 8);
        w.bits(4, 4);
        w.bits(8, 4);
        w.bits(0, 1);
        w.bits(0, 1);

        w.u32(self.network_tick);

        w.bits(self.object_packs.len() as i32, 11);
        w.bits(self.pack_offset as i32, 11);
        for pack in &self.object_packs {
            pack.write(w);
        }

        w.bits(0, 8);
        w.bits(0, 8);

        for human in &self.humans {
            human.write_body(w);
        }
        for item in &self.items {
            item.write_body(w);
        }

        w.bits(self.vehicles.len() as i32, 8);
        for vehicle in &self.vehicles {
            vehicle.write_body(w);
        }

        w.bits(0, 8);
        w.bits(self.traffic_count, 10);
        w.bits(self.traffic.len() as i32, 8);
        let mut next = 0;
        for car in &self.traffic {
            let skip = car.index as i32 - next;
            if skip > 0 {
                w.bits(0, 1);
                w.bits(skip, 10);
            }
            car.write_body(w);
            next = car.index as i32 + 1;
        }
        if self.traffic_count > next {
            w.bits(0, 1);
            w.bits(self.traffic_count - next, 10);
        }

        w.bits(self.signal.0, 10);
        for light in self.signal.1 {
            w.bits(light, 2);
        }

        for voice in &self.voice {
            if let Some(voice) = voice {
                w.bits(1, 1);

                w.bits(voice.player_id, 8);
                w.bits(voice.human_id, 8);
                w.bits(voice.item_id, 8);

                for frame in &voice.voice_frames {
                    w.bits(frame.index as i32, 6);
                    w.bits(frame.size as i32, 11);
                    w.bits(frame.volume as i32, 2);

                    w.bytes(&frame.data);
                }
            } else {
                w.bits(0, 1);
            }
        }

        w.bits(self.global_event_count as i32, 16);
        w.bits(self.events.len() as i32, 6);
        w.bits(self.first_event as i32, 16);
        for (_, e) in &self.events {
            e.write(w)
        }

        w.u32(self.network_tick);
        w.u32(self.network_tick);
        w.u32(self.last_sdl_tick);
    }
}

pub fn get_sun_time(hour: i32, minute: i32) -> i32 {
    let hour_time: i32 = hour.clamp(0, 24) * 216000;
    let minute_time: i32 = minute.clamp(0, 59) * 3600;

    hour_time + minute_time
}