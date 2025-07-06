use crate::packets::{buf_writer::AlexBufWriter, Encodable};

#[derive(Clone)]
pub struct EventChat {
    pub tick_created: i32,
    pub message_type: i32,
    pub message: String,
    pub speaker_id: i32,
    pub volume: i32
}

impl Encodable for EventChat {
    fn encode(&self, _state: &crate::AppState) -> Vec<u8> {
        let mut writer = AlexBufWriter::new();

        writer.write_bits(2, 6);
        writer.write_bits(self.tick_created, 28);

        let mut message = self.message.clone();
        message.push('\0');

        writer.write_bits(message.len() as i32, 6);
        writer.write_bits(self.message_type, 4);
        writer.write_bits(self.speaker_id, 10);
        writer.write_bits(self.volume, 4);

        for char in message.bytes() {
            writer.write_bits(char as i32, 7);
        }

        writer.into_vec()
    }
}