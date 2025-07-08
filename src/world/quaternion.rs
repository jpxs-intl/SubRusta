use crate::{connection::packets::{Encodable, WriterEncodable}, packets::{buf_writer::AlexBufWriter, StatelessEncodable}, world::{matrix::RotMatrix, vector::Vector}};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Quaternion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl StatelessEncodable for Quaternion {
    fn encode(&self) -> Vec<u8> {
        let mut writer = AlexBufWriter::new();

        writer.write_bits(self.w as i32, 2);
        writer.write_delta_rot(0, self.y as i32, false, 14);
        writer.write_delta_rot(0, self.z as i32, false, 14);
        writer.write_delta_rot(0, self.x as i32, false, 14);

        writer.into_vec()
    }
}

impl Quaternion {
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    pub fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 0.0
        }
    }

    pub fn scale(&mut self, scale: f32) {
        self.x *= scale;
        self.y *= scale;
        self.z *= scale;
    }

    pub fn identity() -> Self {
        Self::new(0.0, 0.0, 0.0, 1.0)
    }
    
    pub fn magnitude(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt()
    }

    pub fn magnitude_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w
    }

    pub fn normalized(&self) -> Self {
        let mag = self.magnitude();
        if mag < f32::EPSILON {
            Self::identity()
        } else {
            Self::new(self.x / mag, self.y / mag, self.z / mag, self.w / mag)
        }
    }

    pub fn look_at(&self, pos: Vector, dst: Vector) -> Quaternion {
        let up = Vector::up();
        let forward = (pos - dst).normalized();
        
        let forward = forward.normalized();
        let up = up.normalized();

        if forward.magnitude_squared() < f32::EPSILON {
            return Quaternion::identity();
        }

        let right = up.cross(&forward).normalized();

        if right.magnitude_squared() < f32::EPSILON {
            let right = if forward.dot(&Vector::right()).abs() < 0.9 {
                Vector::right().cross(&forward).normalized()
            } else {
                Vector::up().cross(&forward).normalized()
            };

            let up = forward.cross(&right);

            let matrix = RotMatrix::from_columns(right, up, forward);
            return matrix.to_quaternion().normalized();
        }

        let up = forward.cross(&right);

        let matrix = RotMatrix::from_columns(right, up, forward);
        matrix.to_quaternion().normalized()
    }
}