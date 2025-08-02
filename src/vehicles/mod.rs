use dashmap::DashMap;

use crate::{connection::packets::buf_writer::AlexBufWriter, world::transform::Transform};

#[derive(Default)]
pub struct VehicleManager {
    pub vehicles: DashMap<u32, Vehicle>
}

impl VehicleManager {
    pub fn new() -> Self {
        Self {
            vehicles: DashMap::new()
        }
    }
}

pub struct Vehicle {
    pub vehicle_id: u32,
    pub engine_rpm: u16,
    pub transform: Transform
}

impl Vehicle {
    pub fn encode_obj(&self, writer: &mut AlexBufWriter) {
        writer.write_bits(self.vehicle_id as i32, 10);
        writer.write_bits(0, 2);
        writer.write_bits(0, 10);

        writer.write_bits(self.vehicle_id as i32, 8);

        self.transform.pos.encode_delta(writer);

        self.transform.rot.encode_xyz(writer);

        writer.write_delta_rot(0, 0, false, 9);

        for _ in 0..4 {
            writer.write_delta_pos(0, 0, false, 8);
            writer.write_delta_rot(0, 0, false, 9);
            writer.write_delta_pos(0, 0, false, 8);
        }

        writer.write_bits(self.engine_rpm as i32, 13);
    }
}