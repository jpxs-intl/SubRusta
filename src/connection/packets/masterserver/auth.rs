use std::{net::SocketAddr, sync::Arc};

use crate::{app_state::AppState, packets::{buf_reader::AlexBufReader, Decodable}};

#[derive(Debug, Clone, PartialEq)]
pub struct MasterServerAuthPacket {
    pub account_id: u32,
    pub phone_number: u32,
    pub steam_id: u64,
    pub auth_ticket: u32,
    pub name: String,
}

impl Decodable for MasterServerAuthPacket {
    fn decode(buf: Vec<u8>, _src: SocketAddr, _state: &Arc<AppState>) -> Option<Self> {
        let mut reader = AlexBufReader::from_buf(buf);

        let account_id = reader.read_u32()?;
        let phone_number = reader.read_u32()?;
        let steam_id = u64::from_le_bytes(reader.read_bytes(8, 1)?.try_into().unwrap());
        let auth_number = reader.read_u32()?;

        let name = reader.read_string(32)?;

        Some(MasterServerAuthPacket {
            account_id,
            phone_number,
            steam_id,
            auth_ticket: auth_number,
            name,
        })
    }
}
