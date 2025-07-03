use crate::{packets::{buf_reader::AlexBufReader, Decodable}, AppState};

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundInfoRequest {
    pub version: u8,
    pub timestamp: u32,
}

impl Decodable for ServerboundInfoRequest {
    fn decode(buf: Vec<u8>, _state: &AppState) -> Option<Self> {
        let mut reader = AlexBufReader::from_buf(buf);

        let version = reader.read_u8()?;
        let timestamp = reader.read_u32()?;

        Some(ServerboundInfoRequest {
            version,
            timestamp,
        })
    }
}