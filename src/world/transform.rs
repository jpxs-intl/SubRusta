use crate::world::{euler_rot::EulerRot, quaternion::Quaternion, vector::Vector};

pub struct Transform {
    pub pos: Vector,
    pub rot: Quaternion   
}

impl Transform {

    pub fn pos_rot(pos: Vector, rot: Quaternion) -> Self {
        Self {
            pos, rot: rot.normalized()
        }
    }

    pub fn rotate(&mut self, x: f32, y: f32, z: f32) {
        let euler = EulerRot {
            x: x.to_degrees(),
            y: y.to_radians(),
            z: z.to_radians()
        }.to_quaternion_zyx().normalized();

        self.rot *= euler;
    }

    pub fn rotate_vector(&mut self, vector: Vector) {
        let euler = EulerRot {
            x: vector.x,
            y: vector.y,
            z: vector.z
        }.to_quaternion_zyx().normalized();

        self.rot *= euler;
    }

    pub fn rotate_around(&mut self, point: Vector, axis: Vector, angle: f32) {
        let relative_pos = self.pos - point;
        let rotation = Quaternion::angle_axis(angle, axis);

        let rotated_pos = rotation * relative_pos;

        self.pos = point + rotated_pos;
        self.rot = rotation * self.rot;
    }
}