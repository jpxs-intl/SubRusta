use crate::codec::{Reader, WireRead};

#[derive(Debug, Clone, PartialEq)]
pub struct JoinRequest {
    pub version: u8,
    pub account_id: u32,
    pub auth_ticket: u32,
    pub player_name: String,
    pub avatar_info: u32, // This is not used, but we keep it to maintain the buffer state
    pub password: String,
    pub phone_number: u32,
    pub protocol_version: u8,
}

impl WireRead for JoinRequest {
    fn read(reader: &mut Reader) -> Result<Self, crate::codec::CodecError> {
        let version = reader.u8()?;
        let account_id = reader.u32()?;
        let auth_ticket = reader.u32()?;
        let player_name = reader.string(32)?;
        let avatar_info = reader.u32()?;
        let password = reader.string(32)?;
        let protocol_version = reader.u8()?;
        let phone_number = reader.u32()?;

        Ok(JoinRequest {
            version,
            account_id,
            auth_ticket,
            player_name,
            avatar_info,
            password,
            phone_number,
            protocol_version
        })
    }
}