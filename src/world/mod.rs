use crate::packets::{buf_writer::AlexBufWriter, StatelessEncodable};

#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
    pub z: f32
}

impl StatelessEncodable for Vector {
    fn encode(&self) -> Vec<u8> {
        let mut writer = AlexBufWriter::new();

        writer.write_bytes(&self.x.to_le_bytes());
        writer.write_bytes(&self.y.to_le_bytes());
        writer.write_bytes(&self.z.to_le_bytes());

        writer.into_vec()
    }
}