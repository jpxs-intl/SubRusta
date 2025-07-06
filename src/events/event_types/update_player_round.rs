use crate::packets::{buf_writer::AlexBufWriter, Encodable};

#[derive(Clone)]
pub struct EventUpdatePlayerRound {
    pub tick_created: i32,
    pub player_id: u32,
    pub money: i32,
    pub stocks: i32,
    pub phone_number: u32
}

impl Encodable for EventUpdatePlayerRound {
    fn encode(&self, _state: &crate::AppState) -> Vec<u8> {
        let mut writer = AlexBufWriter::new();

        writer.write_bits(8, 6);
        writer.write_bits(self.tick_created, 28);
        writer.write_bits(self.player_id as i32, 8);
        writer.write_bytes(&self.money.to_le_bytes());
        writer.write_bytes(&self.stocks.to_le_bytes());
        writer.write_bytes(&self.phone_number.to_le_bytes());

        writer.into_vec()
    }
}