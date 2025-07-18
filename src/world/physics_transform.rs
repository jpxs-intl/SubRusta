use rapier3d::{na, parry::math};

pub struct PhysicsTranform {
    pub pos: math::Vector<f32>,
    pub rot: na::Quaternion<f32>,
    pub vel: math::Vector<f32>
}

impl PhysicsTranform {
    pub fn zero() -> Self {
        Self {
            pos: math::Vector::new(0.0, 0.0, 0.0),
            rot: na::Quaternion::new(1.0, 0.0, 0.0, 0.0),
            vel: math::Vector::new(0.0, 0.0, 0.0)
        }
    }
}