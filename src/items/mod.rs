use dashmap::DashMap;

use crate::{connection::packets::buf_writer::AlexBufWriter, world::Vector};

#[derive(Default)]
pub struct ItemManager {
    pub items: DashMap<u32, Item>
}

pub struct Item {
    pub item_type: u32,
    pub item_id: u32,
    pub pos: Vector
}

impl Item {
    pub fn encode_obj_header(&self, writer: &mut AlexBufWriter) {
        writer.write_bits(self.item_id as i32, 10);
        writer.write_bits(0, 2);
        writer.write_bits(1, 3);
        writer.write_bits(self.item_type as i32, 10);
        writer.write_bits(self.item_id as i32, 16);
    }

    pub fn encode_obj(&self, writer: &mut AlexBufWriter) {
        writer.write_bits(1, 1);
        writer.write_bits(1, 1);

        writer.write_bits(self.item_type as i32, 8);
        writer.write_bits(-1, 10);
        writer.write_bits(-1, 10);
        writer.write_bits(0, 4);
        
        writer.write_bits(self.item_id as i32, 8);

        let x = (self.pos.x + 4096.0) * 4096.0;
        let y = (self.pos.y) * 4096.0;
        let z = (self.pos.z + 4096.0) * 4096.0;

        writer.write_delta_pos(0, x as i32, false, 28);
        writer.write_delta_pos(0, y as i32, false, 28);
        writer.write_delta_pos(0, z as i32, false, 28);

        writer.write_bits(0, 2);

        writer.write_delta_rot(0, 0, 14);
        writer.write_delta_rot(0, 0, 14);
        writer.write_delta_rot(0, 0, 14);
    }
}

impl ItemManager {
    pub fn new() -> Self {
        Self {
            items: DashMap::new()
        }
    }
}