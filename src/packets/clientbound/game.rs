use crate::packets::{buf_writer::AlexBufWriter, Encodable, GameState};

#[derive(Debug, Clone, PartialEq)]
pub struct ClientboundGamePacket {
    pub round_number: u32,
    pub network_tick: u32,
    pub game_state: GameState,

    // If gameState is Intermission
    pub ready_status: Option<[bool; 32]>,

    // if game_state is Intermission or Restarting
    pub corporation_money: Option<ClientboundGamePacketCorporationMoney>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClientboundGamePacketCorporationMoney {
    pub corporation_bonus: u16,
    pub corporation_versus_money: u16
}

impl Encodable for ClientboundGamePacket {
    fn encode(&self, _state: &crate::AppState) -> Vec<u8> {
        let mut writer = AlexBufWriter::new();
    
        writer.write_byte(0x05);
        writer.write_bytes(&self.round_number.to_le_bytes());
        writer.write_bytes(&self.network_tick.to_le_bytes());
        writer.write_bits(self.game_state.clone() as u32, 4);
        
        if self.game_state == GameState::Intermission {
            for status in self.ready_status.unwrap().iter() {
                writer.write_bits(*status as u32, 1);
            }
        }

        writer.into_vec()
    }
}