use crate::codec::WireWrite;

#[derive(Clone, Debug, PartialEq)]
pub struct EventTeamDoor {
    pub team_id: i32,
    pub door_open: bool
}

impl WireWrite for EventTeamDoor {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.bits(self.team_id, 8);
        w.bits(self.door_open as i32, 1);
    }
}