use crate::{clientbound::game::VehicleKind, codec::WireWrite};

#[derive(Clone, Debug, PartialEq)]
pub struct EventUpdateVehicleTypeColor {
    pub vehicle_id: i32,
    pub vehicle_type: VehicleKind,
    pub vehicle_color: u8
}

impl WireWrite for EventUpdateVehicleTypeColor {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.bits(self.vehicle_id, 10);
        w.bits(self.vehicle_type as i32, 8);
        w.bits(self.vehicle_color as i32, 4);
    }
}