use crate::codec::{Reader, WireRead};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AvatarInfo {
    pub gender: u8,
    pub head: u8,
    pub skin_color: u8,
    pub hair_color: u8,
    pub hair: u8,
    pub eye_color: u8
}

impl AvatarInfo {
    pub fn from_avatar_info(info: u32) -> Self {
        Self {
            gender: ( info & 0x1) as u8,
            head: (((info >> 1) & 0xF) as u8).min(4),
            skin_color: (((info >> 5) & 0x7) as u8).min(5),
            hair_color: (((info >> 8) & 0xF) as u8).min(11),
            hair: (((info >> 12) & 0xF) as u8).min(8),
            eye_color: (((info >> 16) & 0xF) as u8).min(7),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JoinRequest {
    pub version: u8,
    pub account_id: u32,
    pub auth_ticket: u32,
    pub player_name: String,
    pub avatar_info: AvatarInfo,
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
            avatar_info: AvatarInfo::from_avatar_info(avatar_info),
            password,
            phone_number,
            protocol_version
        })
    }
}