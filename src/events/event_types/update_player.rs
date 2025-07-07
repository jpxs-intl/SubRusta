use crate::{connection::CharacterCustomization, packets::{buf_writer::AlexBufWriter, utils::limited_string, EncodableEvent, Team}};

#[derive(Clone)]
pub struct EventUpdatePlayer {
    pub tick_created: i32,
    pub client_id: u32,
    pub team: Team,
    pub customization: CharacterCustomization,
    pub active: bool,
    pub is_bot: bool,
    pub human_id: i32,
    pub name: String
}

// TODO: Make these bitfields actually work
impl EncodableEvent for EventUpdatePlayer {
    fn encode(&self, _state: &crate::AppState, writer: &mut AlexBufWriter) {
        writer.write_bits(7, 6);
        writer.write_bits(self.tick_created, 28);

        // Start packing for A
        let team_bits = (self.team as i32 + 1) << 10;
        let active_bits = (self.active as i32) << 9;
        let bot_bits = (self.is_bot as i32) << 8;

        let a = team_bits + active_bits + self.client_id as i32 + bot_bits;
        // -----------

        // Start packing for C
        let gender_bits = self.customization.gender;
        let head_bits = self.customization.head << 1;
        let skin_color_bits = self.customization.skin << 6;
        let hair_bits = self.customization.hair_style << 9;

        let c = gender_bits + head_bits + skin_color_bits + hair_bits;
        // -----------

        // Start packing for D
        let eye_color_bits = self.customization.eye_color;
        let hair_color_bits = self.customization.hair_color << 3;
        let model_bits = self.customization.model << 7;
        let suit_color_bits = self.customization.suit_color << 0xc;
        let tie_color_bits = self.customization.tie_color << 0x10;
        let necklace_bits = self.customization.necklace << 0x14;

        let d1 = eye_color_bits + hair_color_bits + model_bits + suit_color_bits + tie_color_bits;
        let d = necklace_bits + d1;
        // -----------

        writer.write_bits(a, 16);
        writer.write_bits(self.human_id, 10);
        writer.write_bytes(&c.to_le_bytes());
        writer.write_bits(d, 24);

        for char in limited_string(&self.name, 31) {
            writer.write_bits(char as i32, 7);
        }
    }
}