use crate::codec::WireWrite;

/// Event 0xf: which items of a level cell's item set have been taken out as real items, so clients stop drawing them.
#[derive(Clone, Debug, PartialEq)]
pub struct EventItemSetCell {
    pub area: i32,
    pub block_x: i32,
    pub block_y: i32,
    pub block_z: i32,
    pub taken: u32,
    // TODO: name once its readers are ported (always 1)
    pub unk: i32,
}

impl WireWrite for EventItemSetCell {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.i32(self.area + (self.block_x << 16));
        w.i32((self.block_z << 16) + self.block_y);
        w.u32(self.taken);
        w.bits(self.unk, 2);
    }
}
