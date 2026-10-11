use crate::codec::WireWrite;

/// Event 0xc: one of a corporation's missions, packed into four ints, sent only to the corporation's team.
#[derive(Clone, Debug, PartialEq)]
pub struct EventUpdateCorporation {
    /// Active flag, slot << 1, corporation << 4, type << 8, disk type << 16.
    pub a: i32,
    /// First team, second team << 4, value << 8.
    pub b: i32,
    /// Deadline in clock minutes, (location + 1) << 16.
    pub c: i32,
    /// The cash provided for the mission.
    pub d: i32,
}

impl EventUpdateCorporation {
    pub fn corporation(&self) -> i32 {
        self.a >> 4 & 15
    }
}

impl WireWrite for EventUpdateCorporation {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.i32(self.a);
        w.i32(self.b);
        w.i32(self.c);
        w.i32(self.d);
    }
}
