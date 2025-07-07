use crossbeam::channel::{Receiver, Sender};
use std::{array, net::SocketAddr, sync::Arc, time::SystemTime};
use tokio::task::JoinHandle;

use crate::{
    packets::{
        clientbound::game::{self, ClientboundGamePacket, ClientboundGamePacketCorporationMoney}, masterserver::auth::MasterServerAuthPacket, serverbound::{game::actions::ServerboundGameAction, join_request::ServerboundJoinRequest}, Encodable, GameState, PacketType
    }, AppState
};

#[derive(Debug, Clone)]
pub struct ClientConnection {
    pub client_id: u32,
    pub recieved_actions: u32,
    pub last_sdl_tick: u32,
    pub last_ping: u32,
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
    pub fn from_address(
        address: SocketAddr,
        tx_socket: Sender<(Vec<u8>, SocketAddr)>,
        auth: &MasterServerAuthPacket,
        connection_id: u32,
    ) -> Self {
        let (tx_sender, tx_receiver) = crossbeam::channel::unbounded();

        let mut conn = ClientConnection {
            client_id: connection_id,
            recieved_actions: 0,
            last_sdl_tick: 0,
            last_ping: 0,
            tx_socket,
            last_packet: SystemTime::now(),
            address,
            username: auth.name.clone(),
            account_id: auth.account_id,
            phone_number: auth.phone_number,
            tx_sender,
            tx_receiver,
            tx_handle: Arc::new(None),
        };

        let c = conn.clone();
        conn.tx_handle = Arc::new(Some(tokio::spawn(async move { c.do_sending() })));

        conn
    }

    pub fn kill_thread(&self) {
        if let Some(handle) = self.tx_handle.as_ref() {
            handle.abort();
        }
    }

    fn do_sending(&self) {
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
            received_actions: self.recieved_actions,
            last_sdl_tick: self.last_sdl_tick,
            round_number: 1,
            network_tick: state.network_tick(),
            menu_type: 2,
            game_state: GameState::Intermission,
            ready_status: Some(array::from_fn(|_| false)),
            corporation_money: Some(ClientboundGamePacketCorporationMoney {
                corporation_bonus: 0,
                corporation_versus_money: 0,
            }),
        };

        self.send_data(game.encode(state));
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
            let mut event_data = state.events.players.get_mut(&self.client_id).unwrap();

            event_data.recieved_events = game_packet.recieved_events as u32;
            self.recieved_actions += game_packet.actions.len() as u32;
            self.last_sdl_tick = game_packet.sdl_tick;
            self.last_ping = game_packet.packet_count_maybe;

            for event in game_packet.actions.clone().into_iter() {
                if let ServerboundGameAction::Chat(ref chat) = event {
                    println!("Got message {:?}", chat.message);

                    state.events.send_chat(0, &chat.message, self.client_id as i32, chat.volume as i32, state);
                }

                if let ServerboundGameAction::Menu(ref menu) = event {
                    println!("Got menu {:?}", menu);
                }
            }
        }
    }
}
