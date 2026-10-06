use rosa_math::vector::Vector;
use bitflags::bitflags;

pub mod actions;
pub mod voice;

use crate::{codec::{Reader, WireRead}, serverbound::game::{actions::{GameAction, read_actions}, voice::{VoiceData, decode_voice_data}}};

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct InputFlags: u32 {
        const LEFT_MOUSE = 1 << 0;
        const RIGHT_MOUSE = 1 << 1;
        const JUMP = 1 << 2;
        const CROUCH = 1 << 3;
        const SHIFT = 1 << 4;
        const DROP_ITEM = 1 << 5;
        const GRAB = 1 << 11;
        const RELOAD = 1 << 12;
        const SWITCH_HANDS = 1 << 13;
        const DELETE = 1 << 18;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClientGamePacket {
    pub round_num: u32,

    // All of the following fields are 24 bits each.
    pub gear_x: f32,
    pub left_right: f32,
    pub gear_y: f32,
    pub forward_back: f32,
    pub view_yaw_delta: f32,
    pub view_pitch: f32,
    pub free_look_yaw: f32,
    pub free_look_pitch: f32,
    pub view_yaw: f32,
    pub unknown: f32,
    pub view_pitch_delta: f32,

    // Go to next byte if in the middle of one from current written bits
    pub input_flags: u32,
    pub input_type: u8,
    pub zoom_level: u8, // 4 bits
    pub received_events: u32,

    // Go to next byte if in the middle of one from current written bits
    pub num_sent_objects: u32,
    pub camera_pos: Vector,

    pub packet_action_count: u8, // 4 bits
    pub total_actions: u8,       // 8 bits

    pub actions: Vec<GameAction>,
    pub voice_data: VoiceData,

    pub spectating_human_id: u8, // 8 bits
    pub unk: u16,                // 11 bits
    pub unk1: u8,                // 8 bits
    pub packet_count_maybe: u32, // 4 bytes
    pub sdl_tick: u32,           // 4 bytes
}

impl WireRead for ClientGamePacket {
    fn read(r: &mut Reader) -> Result<Self, crate::codec::CodecError> {
        let round_num = r.u32()?;
        let gear_x = r.fixed_float()?;
        let left_right = r.fixed_float()?;
        let gear_y = r.fixed_float()?;
        let forward_back = r.fixed_float()?;
        let view_yaw_delta = r.fixed_float()?;
        let view_pitch = r.fixed_float()?;
        let free_look_yaw = r.fixed_float()?;
        let free_look_pitch = r.fixed_float()?;
        let view_yaw = r.fixed_float()?;
        let unknown = r.fixed_float()?;
        let view_pitch_delta = r.fixed_float()?;

        let input_flags = r.u32()?;
        let input_type = r.bits(8)? as u8;
        let zoom_level = r.bits(4)? as u8;
        let received_events = r.bits(16)?;

        let num_sent_objects = r.u32()?;

        let camera_pos = Vector::new(r.f32()?, r.f32()?, r.f32()?);

        let packet_action_count = r.bits(4)?;
        let total_actions = r.bits(8)?;

        let actions = read_actions(r, packet_action_count)?;

        let voice_data = decode_voice_data(r)?;

        let spectating_human_id = r.bits(8)?;
        let unk = r.bits(11)?;
        let unk1 = r.bits(8)?;
        let packet_count_maybe = r.u32()?;
        let sdl_tick = r.u32()?;

        Ok(Self {
            round_num,
            gear_x,
            left_right,
            gear_y,
            forward_back,
            view_yaw_delta,
            view_pitch,
            free_look_yaw,
            free_look_pitch,
            view_yaw,
            unknown,
            view_pitch_delta,
            input_flags,
            input_type,
            zoom_level,
            received_events,
            num_sent_objects,
            camera_pos,
            packet_action_count: packet_action_count as u8,
            total_actions: total_actions as u8,
            actions,
            voice_data,
            spectating_human_id: spectating_human_id as u8,
            unk: unk as u16,
            unk1: unk1 as u8,
            packet_count_maybe,
            sdl_tick
        })
    }
}