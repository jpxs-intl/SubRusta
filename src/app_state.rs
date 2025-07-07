use std::{
    net::SocketAddr,
    sync::{Arc, Mutex, RwLock},
};

use dashmap::DashMap;

use crate::{
    config::config_main::ConfigMain,
    connection::ClientConnection,
    events::{
        EventManager,
        event_types::{Event, chat::EventChat},
    },
    masterserver::MasterServer,
    packets::{GameState, masterserver::auth::MasterServerAuthPacket},
    srk_parser::SrkData,
};

#[derive(Default)]
pub struct LobbyState {
    pub ready: [bool; 32],
    pub state: GameState,
}

pub struct AppState {
    pub network_tick: RwLock<i32>,
    pub round_number: RwLock<u32>,
    pub map_name: RwLock<String>,
    pub masterserver: MasterServer,
    pub srk_data: Arc<Mutex<SrkData>>,
    pub config: ConfigMain,
    pub events: EventManager,
    pub connections: Arc<DashMap<SocketAddr, ClientConnection>>,
    pub auth_data: Arc<DashMap<u32, MasterServerAuthPacket>>,
    pub game_state: RwLock<LobbyState>,

    pub for_broadcast: RwLock<Vec<Vec<u8>>>,
}

impl AppState {
    pub fn broadcast(&self, data: Vec<u8>) {
        let mut writer = self.for_broadcast.write().unwrap();

        writer.push(data);
    }

    pub fn do_broadcast(&self) {
        let connections = self.connections.iter();
        let broadcast = self.for_broadcast.read().unwrap().clone();

        for connection in connections {
            for broadcast in &broadcast {
                connection.send_data(broadcast.to_vec());
            }
        }

        let mut writer = self.for_broadcast.write().unwrap();
        writer.clear();
    }

    pub fn game_state(&self) -> GameState {
        self.game_state.read().unwrap().state.clone()
    }

    pub fn map_name(&self) -> String {
        self.map_name.read().unwrap().clone()
    }

    pub fn network_tick(&self) -> i32 {
        *self.network_tick.read().unwrap()
    }

    pub fn round_number(&self) -> u32 {
        *self.round_number.read().unwrap()
    }

    pub fn send_chat(&self, message_type: i32, message: &str, speaker_id: i32, volume: i32) {
        let event = Event::Chat(EventChat {
            tick_created: self.network_tick(),
            message: message.to_string(),
            message_type,
            speaker_id,
            volume,
        });

        self.events.emit_globally(event);
    }
}
