use crate::world::{quaternion::Quaternion, vector::Vector};

pub struct RotMatrix {
    pub m: [[f32; 3]; 3]
}

impl RotMatrix {
    pub fn new(m: [[f32; 3]; 3]) -> Self {
        Self { m }
    }
    
    pub fn from_columns(right: Vector, up: Vector, forward: Vector) -> Self {
        Self::new([
            [right.x, up.x, forward.x],
            [right.y, up.y, forward.y],
            [right.z, up.z, forward.z],
        ])
    }

    /// Convert rotation matrix to quaternion
    pub fn to_quaternion(&self) -> Quaternion {
        let trace = self.m[0][0] + self.m[1][1] + self.m[2][2];
        
        if trace > 0.0 {
            let s = (trace + 1.0).sqrt();
            let w = s * 0.5;
            let s = 0.5 / s;
            
            Quaternion::new(
                (self.m[1][2] - self.m[2][1]) * s,
                (self.m[2][0] - self.m[0][2]) * s,
                (self.m[0][1] - self.m[1][0]) * s,
                w,
            )
        } else {
            let i = if self.m[1][1] > self.m[0][0] { 1 } else { 0 };
            let i = if self.m[2][2] > self.m[i][i] { 2 } else { i };
            let j = (i + 1) % 3;
            let k = (i + 2) % 3;
            
            let s = (self.m[i][i] - self.m[j][j] - self.m[k][k] + 1.0).sqrt();
            let mut q = [0.0; 4];
            q[i] = s * 0.5;
            let s = 0.5 / s;
            q[3] = (self.m[j][k] - self.m[k][j]) * s;
            q[j] = (self.m[i][j] + self.m[j][i]) * s;
            q[k] = (self.m[i][k] + self.m[k][i]) * s;
            
            Quaternion::new(q[0], q[1], q[2], q[3])
        }
    }
}