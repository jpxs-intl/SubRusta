use crate::{GameMode, codec::{WireWrite, Writer}};

pub struct ServerInfo {
    pub server_name: String,
    pub timestamp: u32,
    pub current_players: u8,
    pub max_players: u8,
    pub build: u8,
    pub gamemode: GameMode,
    pub address: [u8; 4],
    pub port: u16,
    pub password_protected: bool
}

impl WireWrite for ServerInfo {
    fn write(&self, w: &mut Writer) {
        w.byte(0x01);
        w.byte(self.build);
        w.bytes(&self.timestamp.to_le_bytes());

        w.bits(self.gamemode as i32, 4);
        w.bits(Writer::least_significant(self.current_players) as i32, 4);
        w.bits(Writer::most_significant(self.current_players) as i32, 4);
        w.bits(Writer::least_significant(self.max_players) as i32, 4);
        w.bits(Writer::most_significant(self.max_players) as i32, 4);
        w.bits(9, 4);

        w.string(&self.server_name, 32);
        w.bytes(&80085i32.to_le_bytes());
        w.bytes(&self.address);
        w.bytes(&self.port.to_le_bytes());

        w.bits(self.password_protected as i32, 1);
        w.bits(0x04, 8);
        w.bits(0x47, 8);
    }
}