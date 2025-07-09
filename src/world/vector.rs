use std::ops::{Mul, Neg};

use crate::{packets::buf_writer::AlexBufWriter, world::quaternion::Quaternion};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
    pub z: f32
}

impl Vector {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }
    
    pub fn up() -> Self {
        Self::new(0.0, 1.0, 0.0)
    }
    
    pub fn forward() -> Self {
        Self::new(0.0, 0.0, 1.0)
    }
    
    pub fn right() -> Self {
        Self::new(1.0, 0.0, 0.0)
    }

    pub fn dot(&self, other: &Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn encode(&self, writer: &mut AlexBufWriter) {
        writer.write_bytes(&self.x.to_le_bytes());
        writer.write_bytes(&self.y.to_le_bytes());
        writer.write_bytes(&self.z.to_le_bytes());
    }

    pub fn encode_delta(&self, writer: &mut AlexBufWriter) {
        let x = (self.x + 4096.0) * 4096.0;
        let y = (self.y) * 4096.0;
        let z = (self.z + 4096.0) * 4096.0;

        writer.write_delta_pos(0, x as i32, false, 28);
        writer.write_delta_pos(0, y as i32, false, 28);
        writer.write_delta_pos(0, z as i32, false, 28);
    }

    pub fn cross(&self, other: &Vector) -> Vector {
        Vector {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    pub fn magnitude(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn magnitude_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    pub fn normalized(&self) -> Vector {
        let mag = self.magnitude();
        if mag > f32::EPSILON {
            Self::new(self.x / mag, self.y / mag, self.z / mag)
        } else {
            Self::zero()
        }
    }
}

impl std::ops::Add for Vector {
    type Output = Self;
    
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl std::ops::Sub for Vector {
    type Output = Self;
    
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

impl std::ops::Mul<f32> for Vector {
    type Output = Self;
    
    fn mul(self, scalar: f32) -> Self {
        Self::new(self.x * scalar, self.y * scalar, self.z * scalar)
    }
}

impl std::ops::Div<f32> for Vector {
    type Output = Self;
    
    fn div(self, scalar: f32) -> Self {
        Self::new(self.x / scalar, self.y / scalar, self.z / scalar)
    }
}

impl Mul<Quaternion> for Vector {
    type Output = Self;

    fn mul(self, rotation: Quaternion) -> Self {
        let quat_vec = Vector::new(rotation.x, rotation.y, rotation.z);

        let cross1 = self.cross(&quat_vec);
        let cross2 = self.cross(&cross1);

        Vector {
            x: rotation.x + 2.0 * (rotation.w * cross1.x + cross2.x),
            y: rotation.y + 2.0 * (rotation.w * cross1.y + cross2.y),
            z: rotation.z + 2.0 * (rotation.w * cross1.z + cross2.z)
        }
    }
}

impl Neg for Vector {
    type Output = Vector;

    fn neg(self) -> Self {
        Self {
            x: -self.x,
            y: -self.y,
            z: -self.z
        }
    }
}