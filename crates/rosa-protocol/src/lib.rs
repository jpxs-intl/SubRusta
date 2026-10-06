use serde::{Deserialize, Serialize};

use crate::{codec::{CodecError, Reader, WireRead, WireWrite, Writer}, masterserver::AuthPacket, serverbound::{game::ClientGamePacket, info_request::InfoRequest, join_request::JoinRequest}};

pub mod clientbound;
pub mod serverbound;
pub mod masterserver;
pub mod codec;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Team {
    Goldmen = 0,
    Monsota = 1,
    OXS = 2,
    Nexaco = 3,
    Pentacom = 4,

    Spectator = 17
}

#[derive(Debug, Clone, PartialEq)]
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

#[derive(Debug, Clone, PartialEq)]
pub enum ServerboundPacket {
    InfoRequest(InfoRequest),
    JoinRequest(JoinRequest),
    AuthPacket(AuthPacket),
    LeaveGame,
    GamePacket(Box<ClientGamePacket>)
}

pub fn parse_frame(data: &[u8]) -> Option<(u8, &[u8])> {
    if data.len() < 5 || &data[..4] != b"7DFP" { return None; }

    Some((data[4], &data[5..]))
}

pub fn decode_packet(type_byte: u8, body: &[u8]) -> Result<ServerboundPacket, CodecError> {
    let mut r = Reader::new(body);

    Ok(match type_byte {
        0 => ServerboundPacket::InfoRequest(InfoRequest::read(&mut r)?),
        2 => ServerboundPacket::JoinRequest(JoinRequest::read(&mut r)?),
        4 => ServerboundPacket::GamePacket(Box::new(ClientGamePacket::read(&mut r)?)),
        7 => ServerboundPacket::LeaveGame,
        66 => ServerboundPacket::AuthPacket(AuthPacket::read(&mut r)?),
        other => return Err(CodecError::BadEnum(other as u32))
    })
}

pub fn frame_packet(packet: impl WireWrite) -> Vec<u8> {
    let mut w = Writer::new();
    w.bytes(b"7DFP");
    packet.write(&mut w);

    w.into_vec()
}