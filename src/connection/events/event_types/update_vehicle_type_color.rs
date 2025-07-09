use crate::packets::{buf_writer::AlexBufWriter, WriterEncodable};

#[derive(Clone, Debug)]
pub struct EventUpdateVehicleTypeColor {
    pub tick_created: i32, 
    pub vehicle_id: i32,
    pub vehicle_type: u8,
    pub vehicle_color: u8
}

impl WriterEncodable for EventUpdateVehicleTypeColor {
    fn encode(&self, _state: &crate::AppState, writer: &mut AlexBufWriter) {
        writer.write_bits(3, 6);
        writer.write_bits(self.tick_created, 28);
        writer.write_bits(self.vehicle_id, 10);
        writer.write_bits(self.vehicle_type as i32, 8);
        writer.write_bits(self.vehicle_color as i32, 4);
    }
}