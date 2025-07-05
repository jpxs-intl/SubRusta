use serde::{Deserialize, Serialize};

use crate::{packets::{buf_reader::AlexBufReader, clientbound::{initial_sync::ClientboundInitialSyncPacket, kick::ClientboundKickPacket}, masterserver::auth::MasterServerAuthPacket, serverbound::{game::ServerboundGamePacket, info_request::ServerboundInfoRequest, join_request::ServerboundJoinRequest}}, AppState};

pub mod serverbound;
pub mod clientbound;
pub mod utils;
pub mod buf_reader;
pub mod buf_writer;
pub mod masterserver;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum GameMode {
    Driving = 1,
    Racing = 2,
    Round = 3,
    World = 4,
    Eliminator = 5,
    CoOp = 6,
    Versus = 7,
    None = 8
}

pub fn get_sun_time(hour: i32, minute: i32) -> i32 {
    let hour_time: i32 = hour.clamp(0, 24) * 216000;
    let minute_time: i32 = minute.clamp(0, 59) * 3600;

    hour_time + minute_time
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum GameState {
    Idle = 0,
    Intermission = 1,
    InGame = 2,
    Restarting = 3,
    Paused = 4
}

pub trait Encodable {
    fn encode(&self, state: &AppState) -> Vec<u8>;
}

pub trait Decodable: Sized {
    fn decode(buf: Vec<u8>, state: &AppState) -> Option<Self>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum PacketType {
    Unknown,
    ClientboundKickPacket(ClientboundKickPacket),
    ClientboundInitialSyncPacket(ClientboundInitialSyncPacket),
    ServerboundLeave,
    ServerboundJoinRequest(ServerboundJoinRequest),
    ServerboundInfoRequest(ServerboundInfoRequest),
    ServerboundGamePacket(Box<ServerboundGamePacket>),
    MasterServerAuthPacket(MasterServerAuthPacket)
}

pub fn decode_packet(data: Vec<u8>, state: &AppState) -> Option<PacketType> {
    // This is ALWAYS 7DFP\0
    let mut data = data.clone();
    let _header = data.drain(..4).collect::<Vec<u8>>();
    let packet_type: u8 = data.drain(..1).as_slice().to_vec()[0];

    //println!("Decoding packet type: {packet_type} - With data: {data:?}");

    Some(match packet_type {
        0 => PacketType::ServerboundInfoRequest(ServerboundInfoRequest::decode(data, state)?),
        2 => PacketType::ServerboundJoinRequest(ServerboundJoinRequest::decode(data, state)?),
        7 => PacketType::ServerboundLeave,
        4 => PacketType::ServerboundGamePacket(Box::new(ServerboundGamePacket::decode(data, state)?)),
        b'@' => { PacketType::Unknown }
        66 => PacketType::MasterServerAuthPacket(MasterServerAuthPacket::decode(data, state)?),
        _ => {
            println!("Unknown packet type: {packet_type}");
            println!("Data: {data:?}");
            println!("String: {}", String::from_utf8_lossy(&data));
            PacketType::Unknown
        }
    })
}