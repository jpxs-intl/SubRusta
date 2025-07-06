use crate::packets::{buf_writer::AlexBufWriter, Encodable};

#[derive(Clone)]
pub struct EventUpdateVehicleTypeColor {
    pub tick_created: i32, 
    pub vehicle_id: i32,
    pub vehicle_type: i32,
    pub vehicle_color: i32
}

impl Encodable for EventUpdateVehicleTypeColor {
    fn encode(&self, _state: &crate::AppState) -> Vec<u8> {
        let mut writer = AlexBufWriter::new();

        writer.write_bits(3, 6);
        writer.write_bits(self.tick_created, 28);
        writer.write_bits(self.vehicle_id, 10);
        writer.write_bits(self.vehicle_type, 8);
        writer.write_bits(self.vehicle_color, 4);

        writer.into_vec()
    }
}