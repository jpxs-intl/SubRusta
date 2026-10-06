use crate::codec::{Reader, WireRead, };

pub mod server_addr;

#[derive(Debug, Clone, PartialEq)]
pub struct AuthPacket {
    pub account_id: u32,
    pub phone_number: u32,
    pub steam_id: u64,
    pub auth_ticket: u32,
    pub name: String,
}

impl WireRead for AuthPacket {
    fn read(reader: &mut Reader) -> Result<Self, crate::codec::CodecError> {
        let account_id = reader.u32()?;
        let phone_number = reader.u32()?;
        let steam_id = reader.u64()?;
        let auth_ticket = reader.u32()?;

        let name = reader.string(32)?;

        Ok(AuthPacket {
            account_id,
            phone_number,
            steam_id,
            auth_ticket,
            name
        })
    }
}