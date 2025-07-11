#![feature(try_blocks)]
#![feature(async_trait_bounds)]

use std::{
    net::SocketAddr,
    sync::{Arc, Mutex, RwLock},
    time::SystemTime,
};

use crate::{
    app_state::{AppState, ChatType, GameManager}, config::config_main::ConfigMain, connection::{
        events::{event_types::{update_vehicle_type_color::EventUpdateVehicleTypeColor, Event}, 
            EventManager}
        , packets::{self}, ClientConnection
    }, items::ItemManager, map::Map, masterserver::MasterServer, packets::{
        clientbound::{initial_sync::ClientboundInitialSyncPacket, kick::ClientboundKickPacket, server_info::ServerInfo}, Encodable, PacketType
    }, scheduler::TaskScheduler, srk_parser::SrkData, vehicles::{Vehicle, VehicleManager}, voice::VoiceManager, world::{quaternion::Quaternion, transform::Transform, vector::Vector}
};
use crossbeam::channel::{Sender, unbounded};
use dashmap::DashMap;
use tokio::net::UdpSocket;

extern crate serde_repr;

pub mod app_state;
pub mod commands;
pub mod config;
pub mod connection;
pub mod items;
pub mod masterserver;
pub mod scheduler;
pub mod srk_parser;
pub mod vehicles;
pub mod voice;
pub mod world;
pub mod map;

pub static SERVER_IDENTIFIER: u32 = 80085;
pub const TICKS_PER_SECOND: i32 = 62;

#[derive(Debug, Clone)]
pub struct Connection {
    pub address: SocketAddr,
}

#[tokio::main]
async fn main() {
    let config = ConfigMain::read_from_file();

    let _city = Map::load();

    let mut masterserver = MasterServer::init(&config).await;

    let srk_data = SrkData::read_from_file();

    let state = AppState {
        network_tick: RwLock::new(1),
        round_number: RwLock::new(1),
        map_name: RwLock::new("test2".to_string()),
        masterserver: masterserver.clone(),
        events: EventManager::new(),
        voices: VoiceManager::new(),
        items: ItemManager::new(),
        vehicles: VehicleManager::new(),
        tasks: TaskScheduler::new(),
        srk_data: Arc::new(Mutex::new(srk_data)),
        config: config.clone(),
        connections: DashMap::new(),
        auth_data: DashMap::new(),
        game_state: GameManager::default(),
        for_broadcast: RwLock::new(Vec::new()),
    };

    /*state.events.emit_globally(Event::UpdateVehicle(EventUpdateVehicle { 
        tick_created: state.network_tick(), 
        vehicle_id: 0, 
        vehicle_type: 0, 
        color: 1, 
        pos: Vector { x: 1800.0, y: 82.0, z: 1500.0 }, 
        velocity: Vector::zero()
    }));*/

    state.events.emit_globally(Event::UpdateVehicleTypeColor(EventUpdateVehicleTypeColor {
        tick_created: state.network_tick(),
        vehicle_color: 1,
        vehicle_id: 0,
        vehicle_type: 0,
    }));

    state.vehicles.vehicles.insert(
        0,
        Vehicle {
            vehicle_id: 0,
            engine_rpm: 1200,
            transform: Transform::pos_rot(
                Vector {
                    x: 1800.0,
                    y: 82.0,
                    z: 1500.0,
                },
                Quaternion::euler(0.0, 45.0, 0.0),
            ),
        },
    );

    let socket = UdpSocket::bind(format!("0.0.0.0:{}", config.port)).await.expect("Failed to bind socket");
    let recv_sock = Arc::new(socket);

    println!("[SERVER] Listening on {}", recv_sock.local_addr().unwrap());

    let send_sock = make_sender(recv_sock.clone());

    masterserver.connect(send_sock.clone());

    state.tasks.schedule_task(state.network_tick(), Some(TICKS_PER_SECOND * 16), Box::new(|state: &AppState| {
        state.masterserver.send(vec![b'@']);
    }));

    state.tasks.schedule_task(state.network_tick(), Some(TICKS_PER_SECOND * 10), Box::new(|state: &AppState| {
        state.auth_data.retain(|_, (tick_created, _)| {
            state.network_tick() - *tick_created <= TICKS_PER_SECOND * 10
        });
    }));

    let mut packet_buf = [0; 1024];
    let mut last_tick = SystemTime::now();

    loop {
        // Recieve from our sockets, then decode the packet if it is successfull
        if let Ok((size, src)) = recv_sock.try_recv_from(&mut packet_buf)
            && let Some(packet_type) = packets::decode_packet(packet_buf[..size].to_vec().clone(), src, &state)
        {
            // On our connection lets handle the packet
            if let Some(mut connection) = state.connections.get_mut(&src) {
                connection.last_packet = SystemTime::now();

                connection.handle_packet(packet_type.clone(), &state).await;
            }

            // If its a leave, handle it
            if let PacketType::ServerboundLeave = packet_type
                && let Some(connection) = state.connections.get(&src)
            {
                connection.handle_leave(&state);

                println!("[SERVER] {} left.", connection.username);

                drop(connection);

                state.connections.remove(&src);
            }

            // If its a serverbound info request, lets handle it ourselves so we know what were doing
            if let PacketType::ServerboundInfoRequest(ref request) = packet_type {
                let res = ServerInfo {
                    timestamp: request.timestamp,
                    current_players: state.connections.len() as u8,
                    address: "217.197.220.32".to_string(),
                    build: 0x8e,
                };

                send_packet_to_socket(&send_sock, src, &state, &res).await;
            }

            // Handle the join request
            if let PacketType::ServerboundJoinRequest(ref request) = packet_type
                && let Some(auth_data) = state.auth_data.get(&request.account_id)
                && auth_data.1.auth_ticket == request.auth_ticket
            {
                let (_, auth_data) = auth_data.clone();

                // If the password doesnt match what the client sent, lets just disconnect them.
                if !state.config.server_password.is_empty() && request.password != state.config.server_password {
                    let res = ClientboundKickPacket {
                        reason: "You sent an incorrect password, loser.".to_string(),
                    };

                    send_packet_to_socket(&send_sock, src, &state, &res).await;

                // Valid connection and password is correct.
                } else {
                    println!(
                        "[SERVER] Got connection from {:?} with name {} and auth {} - Sending sync!",
                        src, auth_data.name, request.auth_ticket
                    );

                    let res = ClientboundInitialSyncPacket {
                        round_number: state.round_number(),
                        weekly_enabled: false,
                        weekday: 0,
                        sun_angle: 1000,
                        sun_axial_tilt: 1000,
                        versus_movedelay: None,
                    };

                    {
                        let mut data = state.srk_data.lock().unwrap();
                        data.create_account(&auth_data);
                    }

                    let prev_src = state.get_connection_addr_by_rosa_id(auth_data.account_id);

                    // Socket deduping, if we have a socket with this account id, we reparent the old socket to the new one
                    // Thus kicking the OG client.
                    if let Some(prev_src) = prev_src
                        && let Some(socket) = state.connections.get(&prev_src)
                        && prev_src != src
                    {
                        drop(socket);

                        state.reparent_connection(prev_src, src);

                    // If we have a connection already, then lets just make the client happy and send the initial sync.
                    } else if let Some(connection) = state.connections.get(&src) {
                        connection.send_data(res.encode(&state));

                    // Lets make a new connection from the auth packet, send initial sync and stuff.
                    } else {
                        let connection = ClientConnection::from_auth(src, send_sock.clone(), &auth_data, state.find_empty_slot_id());

                        connection.send_data(res.encode(&state));

                        connection.handle_join(&state);

                        state.send_chat(
                            ChatType::PrivateMessage,
                            "This server is NOT real, and you WILL NOT get into a game.",
                            connection.client_id as i32,
                            0,
                        );

                        state.connections.insert(src, connection);
                    }
                }

                if let Some(connection) = state.connections.get(&src) {
                    connection.update_money(&state);
                    connection.update_player(&state);
                }
            }

            // When the MS sends us an auth packet, add the player to our auth stash so we can 
            // figure out who they are on join
            if let PacketType::MasterServerAuthPacket(ref auth) = packet_type {
                println!(
                    "[MasterServer] Recieved authentication packet for {} with phone #{} - Auth ticket: {}",
                    auth.name, auth.phone_number, auth.auth_ticket
                );

                state.auth_data.insert(auth.account_id, (state.network_tick(), auth.clone()));
            }
        };

        if last_tick.elapsed().unwrap().as_millis() > 16 {
            // Start building game packets so we can send them to players
            for connection in state.connections.iter() {
                connection.send_game_packet(&state);
            }

            // Broadcast packets to players
            state.do_broadcast();

            // Remove disconnected players.
            state.connections.retain(|_, connection| {
                if connection.last_packet.elapsed().unwrap().as_millis() > (10 * 1000) {
                    connection.handle_leave(&state);

                    println!("[SERVER] {} on address {} disconnected.", connection.username, connection.address);

                    false
                } else {
                    true
                }
            });

            // Run tasks
            state.tasks.run_tasks(&state);

            // Increase our current network tick
            let mut network_tick = state.network_tick.write().unwrap();
            *network_tick += 1;

            last_tick = SystemTime::now();
        }
    }
    }

pub async fn send_packet_to_socket(socket: &Sender<(Vec<u8>, SocketAddr)>, address: SocketAddr, state: &AppState, packet: &dyn Encodable) {
    let encoded_packet = packet.encode(state);

    let header = b"7DFP";
    let mut data = Vec::with_capacity(header.len() + encoded_packet.len());
    data.extend_from_slice(header);
    data.extend_from_slice(&encoded_packet[..encoded_packet.len()]);

    socket.send((data, address)).expect("Failed to send packet to channel");
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
