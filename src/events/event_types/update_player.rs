use crate::packets::{buf_writer::AlexBufWriter, utils::limited_string, Encodable};

#[derive(Clone)]
pub struct EventUpdatePlayer {
    pub tick_created: i32,
    pub player_id: u32,
    pub team: i32,
    pub active: bool,
    pub is_bot: bool,
    pub human_id: i32,
    pub gender: i32,
    pub head: i32,
    pub skin: i32,
    pub hair: i32,
    pub eye_color: i32,
    pub hair_color: i32,
    pub model: i32,
    pub suit_color: i32,
    pub tie_color: i32,
    pub necklace: i32,
    pub name: String
}

// TODO: Make these bitfields actually work
impl Encodable for EventUpdatePlayer {
    fn encode(&self, _state: &crate::AppState) -> Vec<u8> {
        let mut writer = AlexBufWriter::new();

        writer.write_bits(7, 6);
        writer.write_bits(self.tick_created, 28);

        // Start packing for A
        let team_bits = (self.team + 1) << 10;
        let active_bits = (self.active as i32) << 9;
        let bot_bits = (self.is_bot as i32) << 8;

        let a = team_bits | active_bits | bot_bits | self.human_id;
        // -----------

        // Start packing for C
        let gender_bits = self.gender;
        let head_bits = self.head << 1;
        let skin_color_bits = self.skin << 6;
        let hair_bits = self.hair << 9;

        let c = gender_bits | head_bits | skin_color_bits | hair_bits;
        // -----------

        // Start packing for D
        let eye_color_bits = self.eye_color;
        let hair_color_bits = self.hair_color << 3;
        let model_bits = self.model << 7;
        let suit_color_bits = self.suit_color << 0xc;
        let tie_color_bits = self.tie_color << 0x10;
        let necklace_bits = self.necklace << 0x14;

        let d = eye_color_bits | hair_color_bits | model_bits | suit_color_bits | tie_color_bits | necklace_bits;
        // -----------

        writer.write_bits(a, 16);
        writer.write_bits(self.human_id, 10);
        writer.write_bytes(&c.to_le_bytes());
        writer.write_bits(d, 24);

        for char in limited_string(&self.name) {
            writer.write_bits(char as i32, 7);
        }

        writer.into_vec()
    }
}