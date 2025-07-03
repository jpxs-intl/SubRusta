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
        writer.write_bits(self.game_state.clone() as i32, 4);

        if self.game_state == GameState::Intermission {
            for _ in 0..32 {
                writer.write_bits(0, 1);
            }
        }

        if self.game_state == GameState::Intermission || self.game_state == GameState::InGame {
            for _ in 0..3 {
                writer.write_bits(0, 16);
                writer.write_bits(0, 16);
            }
        }

        writer.write_bits(0x4b0, 24);
        writer.write_bits(0, 16);
        writer.write_bits(0, 30);

        for _ in 0..5 {
            writer.write_bits(0, 6);
        }

        writer.write_bits(2, 8);
        writer.write_bits(-1, 10);
        
        writer.write_bytes(&10.0_f32.to_le_bytes());
        writer.write_bytes(&10.0_f32.to_le_bytes());
        writer.write_bytes(&10.0_f32.to_le_bytes());

        writer.write_bits(1, 1);
        writer.write_bits(0, 8);
        writer.write_bits(16, 0);

        writer.write_bytes(&1000_i32.to_le_bytes());
        writer.write_bytes(&1000_i32.to_le_bytes());
        writer.write_bytes(&1000_i32.to_le_bytes());
        writer.write_bytes(&1000_i32.to_le_bytes());
        writer.write_bytes(&64_i32.to_le_bytes());

        writer.write_bits(300, 16);
        writer.write_bits(0, 8);
        writer.write_bits(100,10);
        writer.write_bits(0, 6);
        writer.write_bits(0, 8);

        for _ in 0..7 {
            writer.write_bits(0, 4);
        }

        writer.write_bits(0, 8);

        writer.into_vec()
    }
}