use crate::codec::WireWrite;

/// Event 0xd: the share prices of the first five corporations in tenths, two to an int (the second in the high half).
#[derive(Clone, Debug, PartialEq)]
pub struct EventUpdateStock {
    pub prices: [i32; 3],
}

impl WireWrite for EventUpdateStock {
    fn write(&self, w: &mut crate::codec::Writer) {
        for p in self.prices {
            w.i32(p);
        }
    }
}
