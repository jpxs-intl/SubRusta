use crate::{connection::menu::MenuTypes, packets::{buf_writer::AlexBufWriter, get_sun_time, Encodable, EncodableEvent, GameState}};

#[derive(Debug, Clone, PartialEq)]
pub struct ClientboundGamePacket {
    pub client_id: u32,
    pub received_actions: u32,
    pub round_number: u32,
    pub network_tick: i32,
    pub last_sdl_tick: u32,
    pub menu_type: MenuTypes,
    pub money: i32,

    // if game_state is Intermission or Restarting
    pub corporation_money: Option<ClientboundGamePacketCorporationMoney>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClientboundGamePacketCorporationMoney {
    pub corporation_bonus: u16,
    pub corporation_versus_money: u16
}

impl Encodable for ClientboundGamePacket {
    fn encode(&self, state: &crate::AppState) -> Vec<u8> {
        let mut writer = AlexBufWriter::new();
    
        writer.write_byte(0x05);
        writer.write_bytes(&self.round_number.to_le_bytes());
        writer.write_bytes(&self.network_tick.to_le_bytes());

        let gstate = state.game_state();

        writer.write_bits(gstate as i32, 4);

        if gstate == GameState::Intermission {
            let ready = state.game_state.ready.lock().unwrap();

            for ready in *ready {
                writer.write_bits(ready as i32, 1);
            }
        } else if gstate == GameState::Restarting {
            for i in 0..3 {
                writer.write_bits(20 * i, 16); // Individual team bonus money
            }
        }

        if gstate == GameState::Intermission || gstate == GameState::Restarting {
            for i in 0..3 {
                writer.write_bits(10 * i, 16); // Team bonus money
                writer.write_bits(10 * i, 16); // Versus total team money.
            }
        }

        writer.write_bits(7200, 24); // Game Timer
        writer.write_bits(9, 16); // Racing time
        writer.write_bits(get_sun_time(12, 60), 30); // Sun time
        
        for _ in 0..5 {
            writer.write_bits(0, 6); // Corporation player count, (Goldmen, Monsota, OXS, Nexaco, Pentacom)
        }

        writer.write_bits(self.client_id as i32, 8); // Local player ID (Player ID this packet is going to basically)
        writer.write_bits(-1, 10); // Local human ID (Human ID of the player this packet is going to)

        writer.write_bytes(&0u32.to_le_bytes()); // Head vel X
        writer.write_bytes(&0u32.to_le_bytes()); // Head vel Y
        writer.write_bytes(&0u32.to_le_bytes()); // Head vel Z

        writer.write_bits(0, 1); // Can see manager tab
        writer.write_bits(self.menu_type as i32, 8); // Player menu tab
        writer.write_bits(0, 16);

        writer.write_bytes(&self.money.to_le_bytes()); // Money
        writer.write_bytes(&0u32.to_le_bytes()); // Team Money
        writer.write_bytes(&0u32.to_le_bytes()); // Team Budget
        writer.write_bytes(&0u32.to_le_bytes()); // Corporate Rating
        writer.write_bytes(&24u32.to_le_bytes()); // Criminal Rating

        writer.write_bits(0, 16); // Player spawn timer
        writer.write_bits(self.received_actions as i32, 8); // Player number of actions
        writer.write_bits(0, 10); // Player human oldHealth
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

        writer.write_bits(0, 8); // Text count
        writer.write_bits(0, 8); // Text offset
        
        // For each enabled or just-now enabled unpack slot bits

        writer.write_bits(0, 8); // Object count

        writer.write_bits(0, 8);
        writer.write_bits(0, 10); // Num of cars
        writer.write_bits(0, 8);        

        writer.write_bits(0, 10);
        writer.write_bits(2, 2);
        writer.write_bits(2, 2);
        writer.write_bits(3, 2);
        writer.write_bits(1, 2);

        let voices = state.voices.client_voices.iter();

        let mut wrote_count = 0;
        for voice in voices {
            if voice.enabled && wrote_count < 8 {
                writer.write_bits(1, 1);
                writer.write_bits(voice.client_id as i32, 8);
                writer.write_bits(-1, 8);
                writer.write_bits(-1, 8);
                
                for i in 0..4 {
                    let frame = &voice.frames[i];

                    writer.write_bits(frame.index as i32, 6);
                    writer.write_bits(frame.size as i32, 11);
                    writer.write_bits(frame.volume as i32, 2);

                    writer.write_bytes(&frame.data);
                }

                wrote_count += 1;
            }
        }

        for _ in wrote_count..8 {
            writer.write_bits(0, 1); // Voice is active
        }

        writer.write_bits(state.events.num_global_events() as i32, 16); // Total number of server events

        let missing = state.events.get_client_missing_events(self.client_id);
        writer.write_bits(missing.len() as i32, 6); // Packed server event count

        if !missing.is_empty() {
            writer.write_bits(missing[0].0 as i32, 16); // starting event id

            for event in missing {
                event.1.encode(state, &mut writer);
            }
        } else {
            writer.write_bits(0, 16);
        }

        writer.write_bytes(&self.network_tick.to_le_bytes()); // ???
        writer.write_bytes(&self.network_tick.to_le_bytes()); // ???
        writer.write_bytes(&self.last_sdl_tick.to_le_bytes()); // Last client SDL tick

        writer.into_vec()
    }
}