use std::sync::Arc;

use crate::{app_state::AppState, packets::{buf_writer::AlexBufWriter, WriterEncodable}, world::vector::Vector};

#[derive(Clone, Debug)]
pub struct EventUpdateVehicle {
    pub tick_created: i32,
    pub vehicle_id: i32,
    pub vehicle_type: i32,
    pub color: i32,
    pub pos: Vector,
    pub velocity: Vector
}

impl WriterEncodable for EventUpdateVehicle {
    fn encode(&self, _state: &Arc<AppState>, writer: &mut AlexBufWriter) {
        writer.write_bits(4, 6);
        writer.write_bits(self.tick_created, 28);
        writer.write_bits(self.vehicle_id, 10);
        writer.write_bits(self.vehicle_type, 4);
        writer.write_bits(self.color, 10);
        self.pos.encode(writer);
        self.velocity.encode(writer);
    }
}