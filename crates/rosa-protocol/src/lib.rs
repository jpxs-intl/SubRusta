use serde::{Deserialize, Serialize};

use crate::{codec::{CodecError, Reader, WireRead}, masterserver::AuthPacket, serverbound::{info_request::InfoRequest, join_request::JoinRequest}};

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

#[derive(Debug, Clone, PartialEq)]
pub enum ServerboundPacket {
    InfoRequest(InfoRequest),
    JoinRequest(JoinRequest),
    AuthPacket(AuthPacket)
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
        66 => ServerboundPacket::AuthPacket(AuthPacket::read(&mut r)?),
        other => return Err(CodecError::BadEnum(other as u32))
    })
}