use crate::packets::{buf_writer::AlexBufWriter, Encodable, GameMode};

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
        let mut writer = AlexBufWriter::new();

        writer.write_byte(0x06);
        writer.write_bytes(&self.round_number.to_le_bytes());
        writer.write_bits(state.config.gamemode.clone() as i32, 4);
        writer.write_bits(self.weekly_enabled as i32, 4);
        writer.write_byte(self.weekday);

        writer.write_string("round".to_string());

        writer.write_bytes(&self.sun_angle.to_le_bytes());
        writer.write_bytes(&self.sun_axial_tilt.to_le_bytes());

        if state.config.gamemode == GameMode::Versus {
            writer.write_byte(self.versus_movedelay.unwrap_or(0));
        }

        writer.into_vec()
    }
}