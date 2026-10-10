use rosa_math::vector::Vector;

use crate::codec::WireWrite;

pub mod server_info;
pub mod initial_sync;
pub mod kick;
pub mod admin_list;
pub mod game;

impl WireWrite for Vector {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.f32(self.0.x);
        w.f32(self.0.y);
        w.f32(self.0.z);
    }
}