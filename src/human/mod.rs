use std::sync::Arc;

use crate::{app_state::AppState, connection::{packets::{buf_writer::AlexBufWriter, clientbound::game::encoding_slots::EncodingSlot}, CharacterCustomization}, world::{quaternion::Quaternion, vector::Vector}};

pub struct Human {
    pub client_id: u32,
    pub human_id: u32,
    pub view_yaw: f32,
    pub view_pitch: f32,
    
    pub standing: bool,
    pub damage: u8,

    pub progress_bar: u8,
    
    pub overall_health: u8,
    pub chest_health: u8,
    pub head_health: u8,
    pub left_arm_health: u8,
    pub right_arm_health: u8,
    pub left_leg_health: u8,
    pub right_leg_health: u8,

    pub stamina: u8,
    pub max_stamina: u8,
    pub customizations: CharacterCustomization,
    pub pos: Vector,
    pub rot: Quaternion
}

impl Human {
    pub fn create(&self, state: &Arc<AppState>) {
        state.encoding_slots.slots.insert(state.encoding_slots.get_slot(), EncodingSlot::Human(self.human_id));
    }

    pub fn encode_header(&self, slot: i32, writer: &mut AlexBufWriter) {
        writer.write_bits(slot, 10);
        writer.write_bits(0, 2);
        writer.write_bits(0, 3);
        writer.write_bits(0, 10);
        writer.write_bits(0, 16);
    }

    pub fn encode_local(&self, _state: &Arc<AppState>, writer: &mut AlexBufWriter) {
        writer.write_bytes(&self.view_yaw.to_le_bytes());
        writer.write_bytes(&self.view_pitch.to_le_bytes());
        writer.write_bytes(&0f32.to_le_bytes());
        writer.write_bytes(&0f32.to_le_bytes());
        writer.write_bytes(&0f32.to_le_bytes());

        writer.write_bits(self.standing as i32, 1);
        writer.write_bits(self.damage as i32, 8);

        writer.write_bits(-1, 8);
        writer.write_bits(0, 7);
        writer.write_bits(0, 4);
        writer.write_bits(0, 4);
        writer.write_bits(self.progress_bar as i32, 8);

        writer.write_bits(self.overall_health as i32, 7);
        writer.write_bits(self.chest_health as i32, 7);
        writer.write_bits(self.head_health as i32, 7);
        writer.write_bits(self.left_arm_health as i32, 7);
        writer.write_bits(self.right_arm_health as i32, 7);
        writer.write_bits(self.left_leg_health as i32, 7);
        writer.write_bits(self.right_leg_health as i32, 7);

        writer.write_bits(self.stamina as i32, 8);
        writer.write_bits(self.max_stamina as i32, 8);
    }

    pub fn encode_slot(&self, _state: &Arc<AppState>, writer: &mut AlexBufWriter) {
        writer.write_bits(1, 1);
        writer.write_bits(1, 1);

        writer.write_bits(-1, 8);
        writer.write_bits(self.customizations.gender, 1);
        writer.write_bits(self.customizations.head, 5);
        writer.write_bits(self.customizations.skin, 3);
        writer.write_bits(self.customizations.hair_style, 4);
        writer.write_bits(self.customizations.eye_color, 3);
        writer.write_bits(self.customizations.hair_color, 4);
        writer.write_bits(1, 5);
        writer.write_bits(self.customizations.suit_color, 4);
        writer.write_bits(self.customizations.tie_color, 4);
        writer.write_bits(self.customizations.necklace, 4);
        writer.write_bits(1, 2);

        writer.write_bits(1, 8);
        writer.write_bits(-1, 8);
        writer.write_bits(0, 4);
        writer.write_bits(1, 1);
        writer.write_bits(0, 1);

        self.pos.encode_delta(writer);
        self.rot.encode_xyz(writer);

        for _ in 0..16 {
            writer.write_bits(1, 1);
            Quaternion::zero().encode_xyz(writer);
        }
    }
}