use crate::codec::WireWrite;

/// Event 0x15: a mission message for one player: their eliminator role at the start, a position to go to, or the
/// results at the end.
#[derive(Clone, Debug, PartialEq)]
pub struct EventMission {
    /// The player it is for (only they are sent it).
    pub player: i32,
    /// 0 the role, 1 a position, 2 a group's result.
    pub kind: i32,
    pub role: i32,
    /// Player ids packed ten bits apart.
    pub value: i32,
    pub pos: [f32; 3],
}

impl WireWrite for EventMission {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.bits(self.player, 10);
        w.bits(self.kind, 4);
        w.bits(self.role, 4);
        w.bits(self.value, 30);
        for v in self.pos {
            w.f32(v);
        }
    }
}
