use rosa_math::vector::Vector;

use crate::codec::WireWrite;

/// Event 0x14: an explosion (size 0 for a grenade).
#[derive(Clone, Debug, PartialEq)]
pub struct EventExplosion {
    pub size: i32,
    pub pos: Vector,
}

impl WireWrite for EventExplosion {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.bits(self.size, 8);
        w.f32(self.pos.0.x);
        w.f32(self.pos.0.y);
        w.f32(self.pos.0.z);
    }
}
