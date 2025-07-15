use std::{
    net::SocketAddr,
    sync::{Arc, Mutex, RwLock},
};

use dashmap::DashMap;

use crate::{
    config::config_main::ConfigMain, connection::{events::{
        event_types::{chat::EventChat, Event}, EventManager
    }, ClientConnection}, items::ItemManager, map::Map, masterserver::MasterServer, packets::{masterserver::auth::MasterServerAuthPacket, GameState}, physics::PhysicsManager, scheduler::TaskScheduler, srk_parser::SrkData, vehicles::VehicleManager, voice::VoiceManager
};

#[derive(Default)]
pub struct GameManager {
    pub ready: Mutex<[bool; 32]>,
    pub state: RwLock<GameState>,
}

impl GameManager {
    pub fn set_player_ready(&self, player_id: u32, ready: bool) {
        let mut lock = self.ready.lock().unwrap();

        lock[player_id.clamp(0, 32) as usize] = ready 
    }

    pub fn get_player_ready(&self, player_id: u32) -> bool {
        let lock = self.ready.lock().unwrap();

        lock[player_id.clamp(0, 32) as usize]
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ChatType {
    Announce = 0,
    Chat = 1,
    ItemSpeak = 2,
    EliminatorAnnouncement = 3,
    AdminChat = 4,
    PrivateMessage = 6
}

pub struct AppState {
    pub network_tick: RwLock<i32>,
    pub round_number: RwLock<u32>,
    pub map_name: RwLock<String>,
    pub masterserver: MasterServer,
    pub srk_data: Arc<Mutex<SrkData>>,
    pub config: ConfigMain,
    pub events: EventManager,
    pub voices: VoiceManager,
    pub items: ItemManager,
    pub vehicles: VehicleManager,
    pub tasks: TaskScheduler,
    pub map: Map,
    pub connections: DashMap<SocketAddr, ClientConnection>,
    pub auth_data: DashMap<u32, (i32, MasterServerAuthPacket)>,
    pub game_state: GameManager,
    pub physics: PhysicsManager,

    pub for_broadcast: RwLock<Vec<Vec<u8>>>,
}

impl AppState {
    pub fn broadcast_packet(&self, data: Vec<u8>) {
        let mut writer = self.for_broadcast.write().unwrap();

        writer.push(data);
    }

    pub fn next_player_id(&self) -> u32 {
        for i in 0..64 {
            if !self.events.players.contains_key(&i) {
                return i;
            }
        }

        0
    }

    pub fn reparent_connection(&self, src: SocketAddr, dst: SocketAddr) {
        if self.connections.contains_key(&src) && !self.connections.contains_key(&dst) {
            let src = self.connections.remove(&src);

            let mut new = src.unwrap().1;
            new.address = dst;

            new.start_read_thread();

            self.connections.insert(dst, new);
        }
    }

    pub fn get_connection_addr_by_rosa_id(&self, account_id: u32) -> Option<SocketAddr> {
        for conn in self.connections.iter() {
            if conn.account_id == account_id {
                return Some(conn.address);
            }
        }

        None
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
        *self.game_state.state.read().unwrap()
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

    pub fn send_chat(&self, chat_type: ChatType, message: &str, speaker_id: i32, volume: i32) {
        let event = Event::Chat(EventChat {
            tick_created: self.network_tick(),
            message: message.to_string(),
            chat_type,
            speaker_id,
            volume,
        });

        self.events.emit_globally(event);
    }
}
