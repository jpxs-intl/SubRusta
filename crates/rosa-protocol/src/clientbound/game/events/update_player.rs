use crate::{CharacterCustomization, Team, codec::WireWrite};

#[derive(Clone, Debug, PartialEq)]
pub struct EventUpdatePlayer {
    pub client_id: u32,
    pub team: Team,
    pub customization: CharacterCustomization,
    pub active: bool,
    pub is_bot: bool,
    pub human_id: i32,
    pub name: String
}

impl WireWrite for EventUpdatePlayer {
    fn write(&self, w: &mut crate::codec::Writer) {
        // Start packing for A
        let team_bits = (self.team as i32 + 1) << 10;
        let active_bits = (self.active as i32) << 9;
        let bot_bits = (self.is_bot as i32) << 8;

        let a = team_bits + active_bits + self.client_id as i32 + bot_bits;
        // -----------

        // Start packing for C
        let gender_bits = self.customization.gender as i32;
        let head_bits = (self.customization.head as i32) << 1;
        let skin_color_bits = (self.customization.skin as i32) << 6;
        let hair_bits = (self.customization.hair_style as i32) << 9;

        let c = gender_bits + head_bits + skin_color_bits + hair_bits;
        // -----------

        // Start packing for D
        let eye_color_bits = self.customization.eye_color as i32;
        let hair_color_bits = (self.customization.hair_color as i32) << 3;
        let model_bits = (self.customization.model as i32) << 7;
        let suit_color_bits = (self.customization.suit_color as i32) << 0xc;
        let tie_color_bits = (self.customization.tie_color as i32) << 0x10;
        let necklace_bits = (self.customization.necklace as i32) << 0x14;

        let d1 = eye_color_bits + hair_color_bits + model_bits + suit_color_bits + tie_color_bits;
        let d = necklace_bits + d1;
        // -----------

        w.bits(a, 16);
        w.bits(self.human_id, 10);
        w.i32(c);
        w.bits(d, 24);

        for i in 0..31 {
            let b = self.name.as_bytes().get(i).copied().unwrap_or(0);
            w.bits((b & 0x7f) as i32, 7);
        }
    }
}
