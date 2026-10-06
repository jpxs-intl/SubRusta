use rosa_math::vector::Vector;

use crate::codec::WireWrite;

#[derive(Clone, Debug, PartialEq)]
pub struct EventUpdateVehicle {
    pub vehicle_id: i32,
    pub vehicle_type: i32,
    pub color: i32,
    pub pos: Vector,
    pub velocity: Vector
}

impl WireWrite for EventUpdateVehicle {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.bits(self.vehicle_id, 10);
        w.bits(self.vehicle_type, 4);
        w.bits(self.color, 10);
        self.pos.write(w);
        self.velocity.write(w);
    }
}