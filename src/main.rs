#![feature(try_blocks)]

use std::{
    array,
    net::SocketAddr,
    sync::Arc,
    thread::sleep,
    time::{Duration, SystemTime},
};

use crate::{
    config::config_main::ConfigMain,
    connection::ClientConnection,
    masterserver::MasterServer,
    packets::{
        Encodable, GameState, PacketType,
        clientbound::{
            game::{ClientboundGamePacket, ClientboundGamePacketCorporationMoney},
            initial_sync::ClientboundInitialSyncPacket,
            server_info::ServerInfo,
        },
        masterserver::auth::MasterServerAuthPacket,
    },
    srk_parser::SrkData,
};
use crossbeam::channel::{Sender, unbounded};
use dashmap::DashMap;
use tokio::net::UdpSocket;

extern crate serde_repr;

pub mod config;
pub mod connection;
pub mod masterserver;
pub mod packets;
pub mod srk_parser;

pub static SERVER_IDENTIFIER: u32 = 80085;

#[derive(Clone)]
pub struct AppState {
    pub masterserver: MasterServer,
    pub srk_data: SrkData,
    pub config: ConfigMain,
    pub connections: Arc<DashMap<SocketAddr, ClientConnection>>,
    pub auth_data: Arc<DashMap<u32, MasterServerAuthPacket>>,
}

impl AppState {
    pub fn broadcast(&self, data: Vec<u8>) {
        let connections = self.connections.iter();

        for val in connections {
            val.send_data(data.clone());
        }
    }
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub address: SocketAddr,
}

#[tokio::main]
async fn main() {
    let config = ConfigMain::read_from_file();

    let mut masterserver = MasterServer::init(&config).await;

    let srk_data = SrkData::read_from_file();

    let app_state = AppState {
        masterserver: masterserver.clone(),
        srk_data,
        config: config.clone(),
        connections: Arc::new(DashMap::new()),
        auth_data: Arc::new(DashMap::new()),
    };

    let socket = UdpSocket::bind(format!("0.0.0.0:{}", config.port))
        .await
        .expect("Failed to bind socket");
    let recv_sock = Arc::new(socket);

    println!("[SERVER] Listening on {}", recv_sock.local_addr().unwrap());

    let send_sock = make_sender(recv_sock.clone());

    masterserver.connect(send_sock.clone());

    tokio::spawn(async move {
        loop {
            masterserver.send(vec![b'@']).await;

            sleep(Duration::from_secs(16));
        }
    });

    let mut packet_buf = [0; 1024];
    let mut last_tick = SystemTime::now();
    let mut network_tick: u32 = 1;

    loop {
        if let Ok((size, src)) = recv_sock.try_recv_from(&mut packet_buf) && let Some(packet_type) = packets::decode_packet(packet_buf[..size].to_vec().clone(), &app_state) {
            if let Some(mut connection) = app_state.connections.get_mut(&src) {
                connection.last_packet = SystemTime::now();

                connection.handle_packet(packet_type.clone(), &app_state).await;
            }

            if let PacketType::ServerboundLeave = packet_type && let Some(connection) = app_state.connections.get(&src) {
                connection.kill_thread();

                drop(connection);

                app_state.connections.remove(&src);
            }

            if let PacketType::ServerboundInfoRequest(ref request) = packet_type {
                let res = ServerInfo {
                    timestamp: request.timestamp,
                    current_players: app_state.connections.len() as u8,
                    address: "217.197.220.32".to_string(),
                    build: 0x8e,
                };

                send_packet_to_socket(&send_sock, src, &app_state, &res).await;
            }

            if let PacketType::ServerboundJoinRequest(ref request) = packet_type
                && let Some(auth_data) = app_state.auth_data.get(&request.account_id)
                && auth_data.auth_ticket == request.auth_ticket
            {
                println!(
                    "[SERVER] Got connection from {:?} with name {} and auth {} - Sending sync!",
                    src, request.player_name, request.auth_ticket
                );

                let res = ClientboundInitialSyncPacket {
                    round_number: 1,
                    weekly_enabled: false,
                    weekday: 0,
                    map_to_load: "round".to_string(),
                    sun_angle: 1000,
                    sun_axial_tilt: 1000,
                    versus_movedelay: None,
                };

                if let Some(connection) = app_state.connections.get(&src) {
                    connection.send_data(res.encode(&app_state));
                } else {
                    let connection = ClientConnection::from_address(src, send_sock.clone());

                    connection.send_data(res.encode(&app_state));
                    app_state.connections.insert(src, connection);
                }
            }

            if let PacketType::MasterServerAuthPacket(ref auth) = packet_type {
                app_state.auth_data.insert(auth.account_id, auth.clone());
            }
        };

        if last_tick.elapsed().unwrap().as_millis() > 16 {
            let game = ClientboundGamePacket {
                round_number: 1,
                network_tick,
                game_state: GameState::Intermission,
                ready_status: Some(array::from_fn(|_| false)),
                corporation_money: Some(ClientboundGamePacketCorporationMoney {
                    corporation_bonus: 0,
                    corporation_versus_money: 0,
                }),
            };

            app_state.broadcast(game.encode(&app_state));

            let mut to_remove = vec![];
            for connection in app_state.connections.iter() {
                if connection.last_packet.elapsed().unwrap().as_millis() > (30 * 1000) {
                    to_remove.push(connection.address);

                    println!("[SERVER] Player {} disconnected.", connection.address);
                }
            }

            for conn in to_remove {
                app_state.connections.remove(&conn);
            }

            network_tick += 1;
            last_tick = SystemTime::now();
        }
    }
}

pub async fn send_packet_to_socket(
    socket: &Sender<(Vec<u8>, SocketAddr)>,
    address: SocketAddr,
    state: &AppState,
    packet: &dyn Encodable,
) {
    let encoded_packet = packet.encode(state);

    let header = b"7DFP";
    let mut data = Vec::with_capacity(header.len() + encoded_packet.len());
    data.extend_from_slice(header);
    data.extend_from_slice(&encoded_packet[..encoded_packet.len()]);

    socket
        .send((data, address))
        .expect("Failed to send packet to channel");
}

fn make_sender(send_sock: Arc<UdpSocket>) -> Sender<(Vec<u8>, SocketAddr)> {
    let (tx, rx) = unbounded::<(Vec<u8>, SocketAddr)>();

    tokio::spawn(async move {
        loop {
            let data = rx.recv();

            if let Ok(data) = data {
                let _res = send_sock.send_to(&data.0, data.1).await;
            }
        }
    });

    tx
}
