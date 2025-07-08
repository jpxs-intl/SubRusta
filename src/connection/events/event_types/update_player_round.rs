use crate::packets::{buf_writer::AlexBufWriter, WriterEncodable};

#[derive(Clone)]
pub struct EventUpdatePlayerRound {
    pub tick_created: i32,
    pub client_id: u32,
    pub money: i32,
    pub stocks: i32,
    pub phone_number: u32
}

impl WriterEncodable for EventUpdatePlayerRound {
    fn encode(&self, _state: &crate::AppState, writer: &mut AlexBufWriter) {
        writer.write_bits(8, 6);
        writer.write_bits(self.tick_created, 28);
        writer.write_bits(self.client_id as i32, 8);
        writer.write_bytes(&self.money.to_le_bytes());
        writer.write_bytes(&self.stocks.to_le_bytes());
        writer.write_bytes(&self.phone_number.to_le_bytes());
    }
}