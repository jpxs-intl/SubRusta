use crate::packets::{Encodable, GameState};

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
        let mut buf = vec![0x05];
        buf.extend_from_slice(&self.round_number.to_le_bytes());
        buf.extend_from_slice(&self.network_tick.to_le_bytes());
        
        if self.game_state == GameState::Intermission {
            let compressed_readies = self.ready_status.unwrap().iter().enumerate().fold(0u32, |acc, (i, &ready)| {
                acc | ((ready as u32) << i)
            });

            buf.extend_from_slice(&compressed_readies.to_le_bytes());
        }

        buf
    }
}