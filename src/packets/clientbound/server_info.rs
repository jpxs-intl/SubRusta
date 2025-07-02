use crate::{packets::{utils::{least_significant, limited_string, most_significant, broken_bit}, Encodable}, AppState, SERVER_IDENTIFIER};

pub struct ServerInfo {
    pub timestamp: u32,
    pub current_players: u8,
    pub address: String,
    pub build: u8,
}

impl Encodable for ServerInfo {
    fn encode(&self, state: &AppState) -> Vec<u8> {
            let mut buf = vec![0x01];

            buf.push(0x26); // Version 38e
            buf.extend_from_slice(&self.timestamp.to_le_bytes()); // timestamp

            buf.push(broken_bit(state.config.gamemode.clone() as u8, least_significant(self.current_players)));
            buf.push(broken_bit(most_significant(self.current_players), least_significant(state.config.max_players)));
            buf.push(broken_bit(most_significant(state.config.max_players), 0));

            // Server Identification
            buf.extend_from_slice(&limited_string(&state.config.server_name)); // 32 bytes of string data
            buf.extend_from_slice(&SERVER_IDENTIFIER.to_le_bytes()); // Identifier

            // Connection Info
            let bytes = self.address.split('.').map(|s| s.parse::<u8>().unwrap()).collect::<Vec<u8>>();
            buf.extend_from_slice(&bytes);
            buf.extend_from_slice(&state.config.port.to_le_bytes());

            // Next the build is the first 7 and is_passworded is the last bit
            buf.push(0b0000100 << 1 | (!state.config.server_password.is_empty()) as u8 & 0b1); // is_passworded + build
            buf.extend_from_slice(&[0x8e, 0x00]);

            buf
    }
}
