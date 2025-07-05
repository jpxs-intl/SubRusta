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
        } else if self.game_state == GameState::Restarting {
            for _ in 0..3 {
                writer.write_bits(0, 16);
            }
        }

        if self.game_state == GameState::Intermission || self.game_state == GameState::Restarting {
            for _ in 0..3 {
                writer.write_bits(0, 16);
                writer.write_bits(0, 16);
            }
        }

        writer.write_bits(32, 24); // Game Timer
        writer.write_bits(9, 16); // Racing time
        writer.write_bits(3000, 30); // Sun time
        
        for _ in 0..6 {
            writer.write_bits(0, 6); // Corporation player count, (Goldmen, Monsota, OXS, Nexaco, Pentacom)
        }

        writer.write_bits(1, 8); // Local player ID (Player ID this packet is going to basically)
        writer.write_bits(-1, 10); // Local human ID (Human ID of the player this packet is going to)

        writer.write_bytes(&0u32.to_le_bytes()); // Head vel X
        writer.write_bytes(&0u32.to_le_bytes()); // Head vel Y
        writer.write_bytes(&0u32.to_le_bytes()); // Head vel Z

        writer.write_bits(0, 1); // Can see manager tab
        writer.write_bits(2, 8); // Player menu tab
        writer.write_bits(0, 16);

        writer.write_bytes(&3000u32.to_le_bytes()); // Money
        writer.write_bytes(&0u32.to_le_bytes()); // Team Money
        writer.write_bytes(&0u32.to_le_bytes()); // Team Budget
        writer.write_bytes(&0u32.to_le_bytes()); // Corporate Rating
        writer.write_bytes(&0u32.to_le_bytes()); // Criminal Rating

        writer.write_bits(16, 16); // Player spawn timer
        writer.write_bits(1, 8); // Player number of actions
        writer.write_bits(10, 10); // Player human oldHealth
        writer.write_bits(0, 6); // Always 0 (thanks alex)
        writer.write_bits(0, 8); // Always 0 (thanks alex)

        for _ in 0..7 {
            writer.write_bits(0, 4); // Item slot number of items
        }

        writer.write_bits(0, 8); // Number of menu buttons

        writer.write_bits(0, 8); // Seemingly unused, idk.

        writer.write_bits(4, 4); // Delta compression param 0 (default number of bits for comp deltas)
        writer.write_bits(8, 4); // Delta compression param 1 (secondary default num of bits for comp deltas)
        writer.write_bits(0, 1); // Unused delta compression debug
        writer.write_bits(0, 1); // Delta compression debug mode (adds an extra flag bit for each signed delta, which returns 0 early.)

        writer.write_bytes(&self.network_tick.to_le_bytes()); // Sent object packets
        writer.write_bits(0, 11); // Packed object count
        writer.write_bits(0, 11); // Packed object offset

        writer.write_bits(0, 11); // Text count
        writer.write_bits(0, 11); // Text offset
        
        // For each enabled or just-now enabled unpack slot bits

        writer.write_bits(0, 8); // Object count

        writer.write_bits(0, 8);
        writer.write_bits(0, 10); // Num of cars
        writer.write_bits(0, 8);        

        writer.write_bits(0, 10);
        writer.write_bits(0, 2);
        writer.write_bits(0, 2);
        writer.write_bits(0, 2);
        writer.write_bits(0, 2);

        for _ in 0..8 {
            writer.write_bits(0, 1); // Voice is active
        }

        writer.write_bits(0, 16); // Total number of server events
        writer.write_bits(0, 6); // Packed server event count
        writer.write_bits(0, 16); // Current event id (num?)

        writer.write_bytes(&0u32.to_le_bytes()); // Client pings?
        writer.write_bytes(&0u32.to_le_bytes()); // ???
        writer.write_bytes(&self.network_tick.to_le_bytes()); // Current SDL tick?

        writer.into_vec()
    }
}