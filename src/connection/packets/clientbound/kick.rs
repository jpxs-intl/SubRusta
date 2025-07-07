use crate::packets::Encodable;

#[derive(Debug, Clone, PartialEq)]
pub struct ClientboundKickPacket {
    pub reason: String
}

impl Encodable for ClientboundKickPacket {
    fn encode(&self, _state: &crate::AppState) -> Vec<u8> {
        if self.reason.len() > 63 {
            panic!("Kick reason exceeds maximum length of 32 bytes");
        }

        let mut buf = vec![];
        
        buf.push(0x03); // header
        
        buf.push(self.reason.len() as u8); // length of the reason string
        buf.extend_from_slice(self.reason.as_bytes()); // reason string

        buf
    }
}