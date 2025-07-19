use std::sync::Arc;

use crate::{app_state::AppState, packets::{buf_writer::AlexBufWriter, WriterEncodable}};

#[derive(Clone, Debug)]
pub struct EventUpdatePhone {
    pub tick_created: i32,
    pub item_id: i32,
    pub phone_status: i32,
    pub display_phone_number: i32,
    pub phone_texture: i32
}

impl WriterEncodable for EventUpdatePhone {
    fn encode(&self, _state: &Arc<AppState>, writer: &mut AlexBufWriter) {
        writer.write_bits(6, 6);
        writer.write_bits(self.tick_created, 28);

        writer.write_bits(self.item_id, 10);
        writer.write_bits(self.phone_status, 3);
        writer.write_bits(self.display_phone_number, 10);
        writer.write_bits(self.phone_texture, 2);
    }
}