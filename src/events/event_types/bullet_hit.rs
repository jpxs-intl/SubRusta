use crate::{packets::{buf_writer::AlexBufWriter, Encodable, StatelessEncodable}, world::Vector};

#[derive(Clone)]
pub struct EventBulletHit {
    pub tick_created: i32,
    pub pos: Vector,
    pub normal: Vector,
    pub hit_type: i32,
    pub unk: i32
}

impl Encodable for EventBulletHit {
    fn encode(&self, _state: &crate::AppState) -> Vec<u8> {
        let mut writer = AlexBufWriter::new();

        writer.write_bits(1, 6);
        writer.write_bits(self.tick_created, 28);
        writer.write_bits(self.unk, 4);
        writer.write_bits(self.hit_type, 6);
        writer.write_bytes(&self.pos.encode());
        writer.write_bytes(&self.normal.encode());

        writer.into_vec()
    }
}