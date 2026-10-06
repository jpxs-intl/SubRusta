use crate::codec::{WireWrite, Writer};

#[derive(Debug, Clone, PartialEq)]
pub struct KickClient {
    pub reason: String
}

impl WireWrite for KickClient {
    fn write(&self, writer: &mut Writer) {
        writer.byte(0x03);
        writer.byte(self.reason.len() as u8);
        writer.bytes(self.reason.as_bytes());
    }
}