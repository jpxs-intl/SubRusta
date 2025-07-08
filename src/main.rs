#![feature(try_blocks)]

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex, RwLock},
    thread::sleep,
    time::{Duration, SystemTime},
};

use crate::{
    app_state::{AppState, ChatType, GameManager}, config::config_main::ConfigMain, connection::{events::{event_types::{update_vehicle_type_color::EventUpdateVehicleTypeColor, Event}, EventManager, PlayerEventManager}, packets, ClientConnection}, items::{Item, ItemManager}, masterserver::MasterServer, packets::{
        clientbound::{
            initial_sync::ClientboundInitialSyncPacket, kick::ClientboundKickPacket,
            server_info::ServerInfo,
        }, Encodable, PacketType
    }, srk_parser::SrkData, voice::{PlayerVoice, VoiceManager}, world::{quaternion::Quaternion, vector::Vector}
};
use crossbeam::channel::{Sender, unbounded};
use dashmap::DashMap;
use tokio::net::UdpSocket;

extern crate serde_repr;

pub mod app_state;
pub mod commands;
pub mod config;
pub mod connection;
pub mod masterserver;
pub mod srk_parser;
pub mod voice;
pub mod world;
pub mod items;

pub static SERVER_IDENTIFIER: u32 = 80085;

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
        network_tick: RwLock::new(0),
        round_number: RwLock::new(1),
        map_name: RwLock::new("test2".to_string()),
        masterserver: masterserver.clone(),
        events: EventManager::new(),
        voices: VoiceManager::new(),
        items: ItemManager::new(),
        srk_data: Arc::new(Mutex::new(srk_data)),
        config: config.clone(),
        connections: DashMap::new(),
        auth_data: DashMap::new(),
        game_state: GameManager::default(),
        for_broadcast: RwLock::new(Vec::new()),
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

    loop {
        if let Ok((size, src)) = recv_sock.try_recv_from(&mut packet_buf)
            && let Some(packet_type) =
                packets::decode_packet(packet_buf[..size].to_vec().clone(), src, &app_state)
        {
            if let Some(mut connection) = app_state.connections.get_mut(&src) {
                connection.last_packet = SystemTime::now();

                connection
                    .handle_packet(packet_type.clone(), &app_state)
                    .await;
            }

            if let PacketType::ServerboundLeave = packet_type
                && let Some(connection) = app_state.connections.get(&src)
            {
                connection.kill_thread();

                app_state.events.players.remove(&connection.client_id);
                app_state.voices.client_voices.remove(&connection.client_id);
                app_state.items.items.remove(&connection.client_id);
                
                println!("[SERVER] {} left!", connection.username);
                app_state.send_chat(ChatType::Announce, &format!("{} left.", connection.username), -1, -1);

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
                if !app_state.config.server_password.is_empty()
                    && request.password != app_state.config.server_password
                {
                    let res = ClientboundKickPacket {
                        reason: "You sent an incorrect password, loser.".to_string(),
                    };

                    send_packet_to_socket(&send_sock, src, &app_state, &res).await;
                } else {
                    println!(
                        "[SERVER] Got connection from {:?} with name {} and auth {} - Sending sync!",
                        src, auth_data.name, request.auth_ticket
                    );

                    let res = ClientboundInitialSyncPacket {
                        round_number: app_state.round_number(),
                        weekly_enabled: false,
                        weekday: 0,
                        sun_angle: 1000,
                        sun_axial_tilt: 1000,
                        versus_movedelay: None,
                    };

                    {
                        let mut data = app_state.srk_data.lock().unwrap();
                        data.create_account(&auth_data);
                    }

                    let prev_src = app_state.get_connection_addr_by_rosa_id(auth_data.account_id);

                    if let Some(prev_src) = prev_src
                        && let Some(socket) = app_state.connections.get(&prev_src) && prev_src != src
                    {
                        drop(socket);

                        app_state.reparent_connection(prev_src, src);
                    } else if let Some(connection) = app_state.connections.get(&src) {
                        connection.send_data(res.encode(&app_state));
                    } else {
                        let connection = ClientConnection::from_auth(
                            src,
                            send_sock.clone(),
                            &auth_data,
                            app_state.find_empty_slot_id(),
                        );

                        connection.send_data(res.encode(&app_state));

                        app_state.events.emit_globally(Event::UpdateVehicleTypeColor(EventUpdateVehicleTypeColor {
                            tick_created: app_state.network_tick(),
                            vehicle_color: 0,
                            vehicle_id: 0,
                            vehicle_type: 0
                        }));

                        app_state.events.players.insert(
                            connection.client_id,
                            PlayerEventManager {
                                player_id: connection.client_id,
                                recieved_events: 0,
                            },
                        );

                        app_state.voices.client_voices.insert(
                            connection.client_id,
                            PlayerVoice {
                                client_id: connection.client_id,
                                enabled: false,
                                frames: vec![],
                            },
                        );

                        app_state.items.items.insert(connection.client_id, Item {
                            item_type: 38,
                            item_id: connection.client_id,
                            pos: Vector::zero(),
                            rot: Quaternion::zero()
                        });

                        app_state.connections.insert(src, connection);
                    }
                }

                if let Some(connection) = app_state.connections.get(&src) {
                    connection.update_money(&app_state);
                    connection.update_player(&app_state);
                }
            }

            if let PacketType::MasterServerAuthPacket(ref auth) = packet_type {
                println!(
                    "[MasterServer] Recieved authentication packet for {} with phone #{} - Auth ticket: {}",
                    auth.name, auth.phone_number, auth.auth_ticket
                );
                app_state.auth_data.insert(auth.account_id, auth.clone());
            }
        };

        if last_tick.elapsed().unwrap().as_millis() > 16 {
            for connection in app_state.connections.iter() {
                connection.send_game_packet(&app_state);
            }

            app_state.do_broadcast();

            let mut to_remove = vec![];
            for connection in app_state.connections.iter() {
                if connection.last_packet.elapsed().unwrap().as_millis() > (30 * 1000) {
                    to_remove.push(connection.address);
                    app_state.events.players.remove(&connection.client_id);
                    app_state.voices.client_voices.remove(&connection.client_id);

                    println!(
                        "[SERVER] {} on address {} disconnected.",
                        connection.username, connection.address
                    );
                }
            }

            for conn in to_remove {
                app_state.connections.remove(&conn);
            }

            let mut network_tick = app_state.network_tick.write().unwrap();
            *network_tick += 1;
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
