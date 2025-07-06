use crate::packets::{buf_writer::AlexBufWriter, Encodable};

#[derive(Clone)]
pub struct EventTeamDoorState {
    pub tick_created: i32,
    pub team_id: i32,
    pub door_open: bool
}

impl Encodable for EventTeamDoorState {
    fn encode(&self, _state: &crate::AppState) -> Vec<u8> {
        let mut writer = AlexBufWriter::new();

        writer.write_bits(10, 6);
        writer.write_bits(self.tick_created, 28);
        writer.write_bits(self.team_id, 8);
        writer.write_bits(self.door_open as i32, 1);

        writer.into_vec()
    }
}