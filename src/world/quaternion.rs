use std::{
    f32::consts::PI,
    f64::consts::FRAC_1_SQRT_2,
    ops::{Mul, MulAssign},
};

use rapier3d::na;

use crate::{
    packets::buf_writer::AlexBufWriter,
    world::{euler_rot::EulerRot, vector::Vector},
};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Quaternion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quaternion {
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }

    pub fn from_rapier(quat: &na::Quaternion<f32>) -> Self {
        Self::new(quat.i, quat.j, quat.k, quat.w).normalized()
    }

    pub fn euler(x: f32, y: f32, z: f32) -> Self {
        let euler = EulerRot {
            x: x.to_radians(),
            y: y.to_radians(),
            z: z.to_radians(),
        };

        euler.to_quaternion_zyx()
    }

    pub fn euler_vector(vector: Vector) -> Self {
        let euler = EulerRot {
            x: vector.x,
            y: vector.y,
            z: vector.z,
        };

        euler.to_quaternion_zyx()
    }

    pub fn pack_data(&self) -> Vec<i32> {
        let mut components = [self.x, self.y, self.z, self.w];
        let abs_components: Vec<f32> = components.iter().map(|&x| x.abs()).collect();

        let largest_component = abs_components
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(index, _)| index)
            .unwrap();

        if components[largest_component] < 0.0 {
            components[0] = -components[0];
            components[1] = -components[1];
            components[2] = -components[2];
            components[3] = -components[3];
        }

        let mut result = Vec::new();

        result.push(largest_component as i32);

        const QUAT_INDEXES: [usize; 4] = [3, 1, 2, 0];

        for &component_index in &QUAT_INDEXES {
            if component_index != largest_component {
                let component_value = components[component_index] as f64;

                let quantized = (component_value * FRAC_1_SQRT_2 * 8191.0) as i32;
                result.push(quantized);
            }
        }

        result
    }

    

    pub fn encode_xyz(&self, writer: &mut AlexBufWriter) {
        let packed = self.pack_data();

        writer.write_bits(packed[0], 2);
        writer.write_delta_rot(0, packed[1], false, 14);
        writer.write_delta_rot(0, packed[2], false, 14);
        writer.write_delta_rot(0, packed[3], false, 14);
    }

    pub fn from_axis_angle(axis_x: f32, axis_y: f32, axis_z: f32, angle_radians: f32) -> Self {
        let half_angle = angle_radians * 0.5;
        let sin_half = half_angle.sin();
        let cos_half = half_angle.cos();

        Quaternion {
            w: cos_half,
            x: axis_x * sin_half,
            y: axis_y * sin_half,
            z: axis_z * sin_half,
        }
    }

    pub fn zero() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        }
    }

    pub fn to_euler_zyx(&self) -> EulerRot {
        let q = self.normalized();

        let sinr_cosp = 2.0 * (q.w * q.x + q.y * q.z);
        let cosr_cosp = 1.0 - 2.0 * (q.x * q.x + q.y * q.y);
        let roll = sinr_cosp.atan2(cosr_cosp);

        let sinp = (1.0 + 2.0 * (q.w * q.y - q.x * q.z)).clamp(-1.0, 1.0).sqrt();
        let cosp = (1.0 - 2.0 * (q.w * q.y - q.x * q.z)).clamp(-1.0, 1.0).sqrt();
        let pitch = 2.0 * sinp.atan2(cosp) - PI / 2.0;

        let siny_cosp = 2.0 * (q.w * q.z + q.x * q.y);
        let cosy_cosp = 1.0 - 2.0 * (q.y * q.y + q.z * q.z);
        let yaw = siny_cosp.atan2(cosy_cosp);

        EulerRot { x: roll, y: pitch, z: yaw }
    }

    pub fn angle_axis(degrees: f32, axis: Vector) -> Self {
        let half_angle = degrees.to_radians() * 0.5;
        let sin_half = half_angle.sin();

        Quaternion {
            w: half_angle.cos(),
            x: axis.x * sin_half,
            y: axis.y * sin_half,
            z: axis.z * sin_half,
        }
    }

    pub fn scale(&mut self, scale: f32) {
        self.x *= scale;
        self.y *= scale;
        self.z *= scale;
    }

    pub fn absolute(&self) -> Quaternion {
        if self.w < 0.0 {
            Quaternion {
                x: -self.x,
                y: -self.y,
                z: -self.z,
                w: -self.w,
            }
        } else {
            *self
        }
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

    pub fn is_valid(&self) -> bool {
        let x = self.x.powf(2.0);
        let y = self.y.powf(2.0);
        let z = self.z.powf(2.0);
        let w = self.w.powf(2.0);

        (x + y + z + w).sqrt() == 1.0
    }

    pub fn normalized(&self) -> Self {
        let mag = self.magnitude();
        if mag < f32::EPSILON {
            Self::identity()
        } else {
            Self::new(self.x / mag, self.y / mag, self.z / mag, self.w / mag)
        }
    }

    pub fn normalize(&mut self) {
        *self = self.clone().normalized();
    }

    pub fn dot(&self, other: &Quaternion) -> f32 {
        self.w * other.w + self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn multiply(&self, other: &Quaternion) -> Quaternion {
        Quaternion {
            w: self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
            x: self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            y: self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            z: self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
        }
    }

    pub fn rotate_vector(&self, v: Vector) -> Vector {
        let quat_vec = Vector::new(self.x, self.y, self.z);
        let cross1 = quat_vec.cross(&v);
        let cross2 = quat_vec.cross(&cross1);
        
        Vector::new(
            v.x + 2.0 * (self.w * cross1.x + cross2.x),
            v.y + 2.0 * (self.w * cross1.y + cross2.y),
            v.z + 2.0 * (self.w * cross1.z + cross2.z),
        )
    }
}

impl Mul<Quaternion> for Quaternion {
    type Output = Quaternion;

    fn mul(self, other: Quaternion) -> Quaternion {
        self.multiply(&other)
    }
}

impl Mul<&Quaternion> for Quaternion {
    type Output = Quaternion;

    fn mul(self, other: &Quaternion) -> Quaternion {
        self.multiply(other)
    }
}

impl Mul<&Quaternion> for &Quaternion {
    type Output = Quaternion;

    fn mul(self, other: &Quaternion) -> Quaternion {
        self.multiply(other)
    }
}

impl Mul<Quaternion> for &Quaternion {
    type Output = Quaternion;

    fn mul(self, other: Quaternion) -> Quaternion {
        self.multiply(&other)
    }
}

impl MulAssign<Quaternion> for Quaternion {
    fn mul_assign(&mut self, rhs: Quaternion) {
        let new = *self * rhs;

        *self = new
    }
}

impl Mul<Vector> for Quaternion {
    type Output = Vector;

    fn mul(self, v: Vector) -> Vector {
        self.rotate_vector(v)
    }
}