use crate::{packets::{buf_writer::AlexBufWriter, utils::{least_significant, most_significant}, Encodable}, AppState, SERVER_IDENTIFIER};

pub struct ServerInfo {
    pub timestamp: u32,
    pub current_players: u8,
    pub address: String,
    pub build: u8,
}

impl Encodable for ServerInfo {
    fn encode(&self, state: &AppState) -> Vec<u8> {
            let mut writer = AlexBufWriter::new();
            writer.write_byte(0x01);
            writer.write_byte(0x26);
            writer.write_bytes(&self.timestamp.to_le_bytes());

            writer.write_bits(state.config.gamemode.clone() as i32, 4);
            writer.write_bits(least_significant(self.current_players) as i32, 4);
            writer.write_bits(most_significant(self.current_players) as i32, 4);
            writer.write_bits(least_significant(state.config.max_players) as i32, 4);
            writer.write_bits(most_significant(state.config.max_players) as i32,4);
            writer.write_bits(9, 4);

            writer.write_string(state.config.server_name.clone());
            writer.write_bytes(&SERVER_IDENTIFIER.to_le_bytes());
            writer.write_bytes(&self.address.split('.').map(|s| s.parse::<u8>().unwrap()).collect::<Vec<u8>>());
            writer.write_bytes(&state.config.port.to_le_bytes());

            writer.write_bits(0b0000100, 7);
            writer.write_bits(!state.config.server_password.is_empty() as i32, 1);

            writer.write_bytes(&[0x8e, 0x00]);

            writer.into_vec()
    }
}
