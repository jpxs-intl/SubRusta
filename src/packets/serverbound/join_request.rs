use crate::{packets::{buf_reader::AlexBufReader, Decodable}, AppState};

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundJoinRequest {
    pub version: u8,
    pub subrosa_id: u32,
    pub auth_ticket: u32,
    pub player_name: String,
    pub avatar_info: u32, // This is not used, but we keep it to maintain the buffer state
    pub password: String,
    pub phone_number: u32,
    pub protocol_version: u8,
}

impl Decodable for ServerboundJoinRequest {
    fn decode(buf: Vec<u8>, _state: &AppState) -> Self {
        let mut reader = AlexBufReader::from_buf(buf);

        let version = reader.read_u8();
        let subrosa_id = reader.read_u32();
        let auth_ticket = reader.read_u32();
        let player_name = reader.read_string(32);
        let avatar_info = reader.read_u32();
        let password = reader.read_string(32);
        let phone_number = reader.read_u32();
        let protocol_version = reader.read_u8();


        ServerboundJoinRequest {
            version,
            subrosa_id,
            auth_ticket,
            player_name,
            password,
            avatar_info,
            phone_number,
            protocol_version
        }
        //ServerboundJoinRequest::from_bytes((buf.as_ref(), 0))
        //    .expect("Failed to decode ServerboundJoinRequest").1
    }
}