use crate::world::quaternion::Quaternion;

#[derive(Debug)]
pub struct EulerRot {
    pub x: f32,
    pub y: f32,
    pub z: f32
}

impl EulerRot {
    pub fn from_degrees(x: f32, y: f32, z: f32) -> Self {
        Self {
            x: x.to_radians(),
            y: y.to_radians(),
            z: z.to_radians()
        }
    }

    pub fn to_euler_degrees(&self) -> EulerRot {
        Self {
            x: self.x.to_radians(),
            y: self.y.to_radians(),
            z: self.z.to_radians()
        }
    }

    pub fn to_quaternion_zyx(&self) -> Quaternion {
        // X-Y-Z rotation order
        let (s_1, c_1) = (self.x * 0.5).sin_cos();
        let (s_2, c_2) = (self.y * 0.5).sin_cos();
        let (s_3, c_3) = (self.z * 0.5).sin_cos();

        Quaternion {
            w: c_1 * c_2 * c_3 + s_1 * s_2 * s_3,
            x: c_1 * c_2 * s_3 - s_1 * s_2 * c_3,
            y: c_1 * s_2 * c_3 + s_1 * c_2 * s_3,
            z: s_1 * c_2 * c_3 - c_1 * s_2 * s_3,
        }.normalized()
    }
}