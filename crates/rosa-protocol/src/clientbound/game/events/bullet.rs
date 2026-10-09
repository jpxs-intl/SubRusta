use rosa_math::vector::Vector;

use crate::codec::WireWrite;

/// Event 0: a gun fired; clients draw the bullet from `pos` along `vel`.
#[derive(Clone, Debug, PartialEq)]
pub struct EventBullet {
    pub bullet_type: i32,
    pub item_id: i32,
    pub pos: Vector,
    pub vel: Vector,
}

impl WireWrite for EventBullet {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.bits(self.bullet_type, 4);
        w.bits(self.item_id, 10);
        self.pos.write(w);
        self.vel.write(w);
    }
}
