use std::sync::Arc;

use crate::{app_state::AppState, packets::{buf_writer::AlexBufWriter, WriterEncodable}};

#[derive(Clone, Debug)]
pub struct EventTeamDoorState {
    pub tick_created: i32,
    pub team_id: i32,
    pub door_open: bool
}

impl WriterEncodable for EventTeamDoorState {
    fn encode(&self, _state: &Arc<AppState>, writer: &mut AlexBufWriter) {
        writer.write_bits(10, 6);
        writer.write_bits(self.tick_created, 28);
        writer.write_bits(self.team_id, 8);
        writer.write_bits(self.door_open as i32, 1);
    }
}