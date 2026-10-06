use crate::codec::WireWrite;

#[derive(Clone, Debug, PartialEq)]
pub struct EventUpdatePhone {
    pub item_id: i32,
    pub phone_status: i32,
    pub display_phone_number: i32,
    pub phone_texture: i32
}

impl WireWrite for EventUpdatePhone {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.bits(self.item_id, 10);
        w.bits(self.phone_status, 3);
        w.bits(self.display_phone_number, 16);
        w.bits(self.phone_texture, 2);
    }
}