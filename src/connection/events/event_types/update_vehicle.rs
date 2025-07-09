use crate::{packets::{buf_writer::AlexBufWriter, WriterEncodable}, world::vector::Vector};

#[derive(Clone)]
pub struct EventUpdateVehicle {
    pub tick_created: i32,
    pub vehicle_id: i32,
    pub update_type: i32,
    pub part_id: i32,
    pub pos: Vector,
    pub normal: Vector
}

impl WriterEncodable for EventUpdateVehicle {
    fn encode(&self, _state: &crate::AppState, writer: &mut AlexBufWriter) {
        writer.write_bits(4, 6);
        writer.write_bits(self.tick_created, 28);
        writer.write_bits(self.vehicle_id, 10);
        writer.write_bits(self.update_type, 4);
        writer.write_bits(self.part_id, 10);
        self.pos.encode(writer);
        self.normal.encode(writer);
    }
}