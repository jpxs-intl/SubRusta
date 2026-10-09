use rosa_math::vector::Vector;

use crate::codec::WireWrite;

#[derive(Clone, Debug, PartialEq)]
pub struct EventUpdateVehicle {
    pub vehicle_id: i32,
    /// What changed: 0 a window broke (or a train event), 1 a tyre burst, 2 the vehicle was wrecked.
    pub kind: i32,
    /// The window or wheel it concerns.
    pub part: i32,
    pub pos: Vector,
    pub velocity: Vector
}

impl WireWrite for EventUpdateVehicle {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.bits(self.vehicle_id, 10);
        w.bits(self.kind, 4);
        w.bits(self.part, 10);
        self.pos.write(w);
        self.velocity.write(w);
    }
}