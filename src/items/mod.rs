use dashmap::DashMap;

use crate::{connection::packets::buf_writer::AlexBufWriter, world::{quaternion::Quaternion, vector::Vector}};

#[derive(Default)]
pub struct ItemManager {
    pub items: DashMap<u32, Item>
}

pub struct Item {
    pub item_type: u32,
    pub item_id: u32,
    pub pos: Vector,
    pub rot: Quaternion
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

        self.pos.encode_delta(writer);

        writer.write_bits(self.rot.w as i32, 2);

        writer.write_delta_rot(0, self.rot.x as i32, false, 14);
        writer.write_delta_rot(0, self.rot.y as i32, false, 14);
        writer.write_delta_rot(0, self.rot.z as i32, false, 14);
    }
}

impl ItemManager {
    pub fn new() -> Self {
        Self {
            items: DashMap::new()
        }
    }
}