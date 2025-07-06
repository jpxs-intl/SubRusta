use crate::packets::{buf_writer::AlexBufWriter, utils::limited_string, Encodable};

#[derive(Clone)]
pub struct EventUpdatePlayer {
    pub tick_created: i32,
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub d: i32,
    pub name: String
}

// TODO: Make these bitfields actually work
impl Encodable for EventUpdatePlayer {
    fn encode(&self, _state: &crate::AppState) -> Vec<u8> {
        let mut writer = AlexBufWriter::new();

        writer.write_bits(7, 6);
        writer.write_bits(self.tick_created, 28);
        writer.write_bits(self.a, 16);
        writer.write_bits(self.b, 10);
        writer.write_bytes(&self.c.to_le_bytes());
        writer.write_bits(self.d, 24);

        for char in limited_string(&self.name) {
            writer.write_bits(char as i32, 7);
        }

        writer.into_vec()
    }
}