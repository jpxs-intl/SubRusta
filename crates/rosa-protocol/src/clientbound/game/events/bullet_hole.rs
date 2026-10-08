use rosa_math::vector::Vector;

use crate::codec::WireWrite;

/// Event 0x10: a breakable face of a level cell broke (a glass pane), so clients remove it.
#[derive(Clone, Debug, PartialEq)]
pub struct EventBulletHole {
    pub area: i32,
    pub block_x: i32,
    pub block_y: i32,
    pub block_z: i32,
    pub cell: u32,
    pub face: i32,
    pub pos: Vector,
    pub vel: Vector,
}

impl WireWrite for EventBulletHole {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.i32(self.area + (self.block_x << 16));
        w.i32((self.block_z << 16) + self.block_y);
        w.u32(self.cell);
        w.bits(self.face, 10);
        self.pos.write(w);
        self.vel.write(w);
    }
}
