use crossbeam::channel::{Receiver, Sender};
use std::{net::SocketAddr, sync::Arc, time::SystemTime};
use tokio::task::JoinHandle;

use crate::{
    app_state::ChatType, commands::parse_command, connection::{
        events::{event_types::{update_player::EventUpdatePlayer, update_player_round::EventUpdatePlayerRound, Event}, PlayerEventManager},
        menu::{enter_city::handle_enter_city_menu_action, lobby::handle_lobby_menu_action, menu_from_num, MenuTypes},
    }, packets::{
        clientbound::game::{ClientboundGamePacket, ClientboundGamePacketCorporationMoney}, masterserver::auth::MasterServerAuthPacket, serverbound::game::actions::ServerboundGameAction, Encodable, PacketType, Team
    }, voice::PlayerVoice, world::vector::Vector, AppState
};

pub mod events;
pub mod menu;
pub mod packets;

#[derive(Debug, Clone)]
pub struct CharacterCustomization {
    pub gender: i32,
    pub head: i32,
    pub skin: i32,
    pub hair_color: i32,
    pub hair_style: i32,
    pub eye_color: i32,
    pub model: i32,
    pub necklace: i32,
    pub suit_color: i32,
    pub tie_color: i32,
}

impl Default for CharacterCustomization {
    fn default() -> Self {
        Self {
            gender: 0,
            head: 4,
            skin: 2,
            hair_color: 4,
            hair_style: 6,
            eye_color: 6,
            model: 1,
            necklace: 0,
            suit_color: 0,
            tie_color: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientConnection {
    pub client_id: u32,
    pub human_id: Option<i32>,
    pub received_actions: u32,
    pub last_sdl_tick: u32,
    pub last_ping: u32,
    pub customization: CharacterCustomization,
    pub team: Team,
    pub money: i32,
    pub menu: MenuTypes,
    pub camera_pos: Vector,

    pub tx_socket: Sender<(Vec<u8>, SocketAddr)>,
    pub last_packet: SystemTime,
    pub address: SocketAddr,
    pub username: String,
    pub account_id: u32,
    pub phone_number: u32,
    tx_sender: Sender<Vec<u8>>,
    tx_receiver: Receiver<Vec<u8>>,
    tx_handle: Arc<Option<JoinHandle<()>>>,
}

impl ClientConnection {
    pub fn from_auth(address: SocketAddr, tx_socket: Sender<(Vec<u8>, SocketAddr)>, auth: &MasterServerAuthPacket, connection_id: u32) -> Self {
        let (tx_sender, tx_receiver) = crossbeam::channel::unbounded();

        let username = if auth.account_id == 1_000_002 {
            "cuckraisefold".to_string()
        } else {
            auth.name.clone()
        };

        let mut conn = ClientConnection {
            client_id: connection_id,
            human_id: None,
            received_actions: 0,
            last_sdl_tick: 0,
            last_ping: 0,
            customization: CharacterCustomization::default(),
            team: Team::Spectator,
            money: 0,
            menu: MenuTypes::Lobby,
            camera_pos: Vector::default(),

            tx_socket,
            last_packet: SystemTime::now(),
            address,
            username,
            account_id: auth.account_id,
            phone_number: auth.phone_number,
            tx_sender,
            tx_receiver,
            tx_handle: Arc::new(None),
        };

        conn.start_read_thread();

        conn
    }

    pub fn handle_join(&self, state: &AppState) {
        state.events.players.insert(
            self.client_id,
            PlayerEventManager {
                player_id: self.client_id,
                recieved_events: 0,
            },
        );

        state.voices.client_voices.insert(
            self.client_id,
            PlayerVoice {
                client_id: self.client_id,
                enabled: false,
                frames: vec![],
            },
        );

        state.send_chat(ChatType::Announce, &format!("{} joined!", self.username), -1, 0);
    }

    pub fn handle_leave(&self, state: &AppState) {
        state.events.players.remove(&self.client_id);
        state.voices.client_voices.remove(&self.client_id);

        self.kill_thread();

        state.send_chat(ChatType::Announce, &format!("{} left.", self.username), -1, 0);
    }

    pub fn start_read_thread(&mut self) {
        self.kill_thread();

        let (tx_sender, tx_receiver) = crossbeam::channel::unbounded();
        self.tx_sender = tx_sender;
        self.tx_receiver = tx_receiver;

        let c = self.clone();
        self.tx_handle = Arc::new(Some(tokio::spawn(async move { c.do_sending() })));
    }

    pub fn kill_thread(&self) {
        if let Some(handle) = self.tx_handle.as_ref() {
            handle.abort();
        }
    }

    pub fn do_sending(&self) {
        loop {
            let bytes = self.tx_receiver.recv();

            if let Ok(bytes) = bytes {
                self.tx_socket.send((bytes, self.address)).unwrap();
            }
        }
    }

    pub fn send_game_packet(&self, state: &AppState) {
        let game = ClientboundGamePacket {
            client_id: self.client_id,
            received_actions: self.received_actions,
            last_sdl_tick: self.last_sdl_tick,
            money: self.money,

            follow_pos: self.camera_pos,

            round_number: state.round_number(),
            network_tick: state.network_tick(),
            menu_type: self.menu,
            corporation_money: Some(ClientboundGamePacketCorporationMoney {
                corporation_bonus: 0,
                corporation_versus_money: 0,
            }),
        };

        self.send_data(game.encode(state));
    }

    pub fn update_player(&self, state: &AppState) {
        let event_update = Event::UpdatePlayer(EventUpdatePlayer {
            tick_created: state.network_tick(),
            client_id: self.client_id,
            active: true,
            customization: self.customization.clone(),
            human_id: self.human_id.unwrap_or(-1),
            is_bot: false,
            team: self.team,
            name: self.username.clone(),
        });

        state.events.emit_globally(event_update);
    }

    pub fn update_money(&self, state: &AppState) {
        let event_update = Event::UpdatePlayerRound(EventUpdatePlayerRound {
            tick_created: state.network_tick(),
            client_id: self.client_id,
            money: self.money,
            phone_number: self.phone_number,
            stocks: 0,
        });

        state.events.emit_globally(event_update);
    }

    pub fn send_data(&self, data: Vec<u8>) {
        let header = b"7DFP";
        let mut vec = Vec::with_capacity(header.len() + data.len());
        vec.extend_from_slice(header);
        vec.extend_from_slice(&data);

        let _ = self.tx_sender.send(vec);
    }

    pub async fn handle_packet(&mut self, packet: PacketType, state: &AppState) {
        if let PacketType::ServerboundGamePacket(ref game_packet) = packet {
            if let Some(mut ev) = state.events.players.get_mut(&self.client_id) {
                ev.recieved_events = game_packet.recieved_events;
            }

            let new_action_amount = game_packet.actions.len() as u32;
            self.received_actions = (self.received_actions + new_action_amount) % 64;
            self.last_sdl_tick = game_packet.sdl_tick;
            self.last_ping = game_packet.packet_count_maybe;
            self.camera_pos = game_packet.camera_pos;

            for event in game_packet.actions.clone().into_iter() {
                if let ServerboundGameAction::Chat(ref chat) = event {
                    println!("{} [>] {}", self.username, chat.message);

                    if !parse_command(self, chat.message.clone(), state) {
                        state.send_chat(ChatType::Announce, &chat.message, self.client_id as i32, chat.volume as i32);
                    }
                }

                if let ServerboundGameAction::Menu(ref menu) = event {
                    let menu_type = menu_from_num(menu.menu);

                    println!("Menu {menu:?} - type {menu_type:?}");

                    match menu_type {
                        MenuTypes::Lobby => handle_lobby_menu_action(menu.button, self, state),
                        MenuTypes::EnterCity => handle_enter_city_menu_action(menu.button, self, state),
                        _ => {}
                    }
                }
            }
        }
    }
}
