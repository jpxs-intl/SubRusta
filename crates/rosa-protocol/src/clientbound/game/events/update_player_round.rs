use crate::codec::WireWrite;

#[derive(Clone, Debug, PartialEq)]
pub struct EventUpdatePlayerRound {
    pub client_id: u32,
    pub money: i32,
    pub stocks: i32,
    pub phone_number: u32
}

impl WireWrite for EventUpdatePlayerRound {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.bits(self.client_id as i32, 8);
        w.i32(self.money);
        w.i32(self.stocks);
        w.u32(self.phone_number)
    }
}
