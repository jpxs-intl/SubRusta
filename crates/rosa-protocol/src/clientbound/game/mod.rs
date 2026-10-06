use rosa_math::vector::Vector;
use serde::{Deserialize, Serialize};

use crate::{clientbound::game::events::Event, codec::{WireWrite, Writer}};

pub mod events;

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
pub struct ServerGamePacket {
    pub client_id: u32,
    pub received_actions: u32,
    pub round_number: u32,
    pub network_tick: u32,
    pub last_sdl_tick: u32,
    pub menu_type: MenuType,
    pub money: i32,
    pub gamestate: GameState,
    pub ready_states: Option<[bool; 32]>,

    pub follow_pos: Vector,

    pub global_event_count: u32,
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

        w.bits(0, 8);
        w.bits(-1, 10);

        w.u32(0);
        w.u32(0);
        w.u32(0);

        w.bits(0, 1);
        w.bits(self.menu_type as i32, 8);
        w.bits(0, 16);

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

        for _ in 0..7 {
            w.bits(0, 4);
        }

        w.bits(0, 8);
        w.bits(0, 8);
        w.bits(4, 4);
        w.bits(8, 4);
        w.bits(0, 1);
        w.bits(0, 1);

        w.u32(self.network_tick);

        w.bits(0, 11);
        w.bits(0, 11);

        w.bits(0, 8);
        w.bits(0, 8);

        w.bits(0, 8);

        w.bits(0, 8);
        w.bits(0, 10);
        w.bits(0, 8);

        w.bits(0, 10);
        w.bits(2, 2);
        w.bits(2, 2);
        w.bits(3, 2);
        w.bits(1, 2);

        for _ in 0..8 {
            w.bits(0, 1);
        }

        w.bits(self.global_event_count as i32, 16);
        w.bits(self.events.len() as i32, 6);

        if !self.events.is_empty() {
            w.bits(self.events.first().unwrap().0 as i32, 16);

            for (_, e) in &self.events {
                e.write(w)
            }
        } else {
            w.bits(0, 16);
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