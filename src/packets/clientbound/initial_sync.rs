use crate::packets::{utils::{limited_string, broken_bit}, Encodable, GameMode};

#[derive(Debug, Clone, PartialEq)]
pub struct ClientboundInitialSyncPacket {
    pub round_number: u32,
    pub weekly_enabled: bool,
    pub weekday: u8,
    pub map_to_load: String,
    pub sun_angle: u16,
    pub sun_axial_tilt: u16,
    pub versus_movedelay: Option<u8>,
}

impl Encodable for ClientboundInitialSyncPacket {
    fn encode(&self, state: &crate::AppState) -> Vec<u8> {
        let mut buf = vec![0x06];
        buf.extend_from_slice(&self.round_number.to_le_bytes());

        buf.push(broken_bit(self.weekly_enabled as u8, state.config.gamemode.clone() as u8));
        //buf.push(broken_bit(state.config.gamemode.clone() as u8, self.weekly_enabled as u8));
        buf.push(self.weekday);
        buf.extend_from_slice(&limited_string(&self.map_to_load));
        buf.extend_from_slice(&self.sun_angle.to_le_bytes());
        buf.extend_from_slice(&self.sun_axial_tilt.to_le_bytes());

        if state.config.gamemode == GameMode::Versus {
            buf.push(self.versus_movedelay.unwrap_or(0));
        }

        buf
    }
}