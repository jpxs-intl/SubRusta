use crate::packets::{
    buf_reader::AlexBufReader, serverbound::game::{actions::{
        decode_actions, ServerboundGameAction
    }, opus::{decode_voice_data, ServerboundGameVoiceData}}, Decodable
};

pub mod actions;
pub mod opus;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundGamePacket {
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
    pub recieved_events: u16,

    // Go to next byte if in the middle of one from current written bits
    pub num_sent_objects: u32,
    pub camera_x: f32,
    pub camera_y: f32,
    pub camera_z: f32,

    pub packet_action_count: u8, // 4 bits
    pub num_actions: u8,         // 8 bits

    pub actions: Vec<ServerboundGameAction>,
    pub voice_data: ServerboundGameVoiceData,

    pub spectating_human_id: u8, // 8 bits
    pub unk: u16,                // 11 bits
    pub unk1: u8,                // 8 bits
    pub unk2: u32,           // 4 bytes
    pub unk3: u32,           // 4 bytes
}

impl Decodable for ServerboundGamePacket {
    fn decode(buf: Vec<u8>, _state: &crate::AppState) -> Self {
        let mut reader = AlexBufReader::from_buf(buf);

        let round_num = reader.read_u32();
        let gear_x = reader.read_special_f32();
        let left_right = reader.read_special_f32();
        let gear_y = reader.read_special_f32();
        let forward_back = reader.read_special_f32();
        let view_yaw_delta = reader.read_special_f32();
        let view_pitch = reader.read_special_f32();
        let free_look_yaw = reader.read_special_f32();
        let free_look_pitch = reader.read_special_f32();
        let view_yaw = reader.read_special_f32();
        let unknown = reader.read_special_f32();
        let view_pitch_delta = reader.read_special_f32();

        let input_flags = reader.read_u32();
        let input_type = reader.boundscheck_read_bits(8) as u8;
        let zoom_level = reader.boundscheck_read_bits(4) as u8;
        let recieved_events = reader.boundscheck_read_bits(16) as u16;

        let num_sent_objects = reader.read_u32();
        let camera_x = reader.read_u32() as f32;
        let camera_y = reader.read_u32() as f32;
        let camera_z = reader.read_u32() as f32;

        let packet_action_count = reader.boundscheck_read_bits(4) as u8;
        let num_actions = reader.boundscheck_read_bits(8);

        let actions = decode_actions(&mut reader, packet_action_count);
        let voice_data = decode_voice_data(&mut reader);

        let spectating_human_id = reader.boundscheck_read_bits(8);

        let unk = reader.boundscheck_read_bits(11);
        let unk1 = reader.boundscheck_read_bits(8);
        let unk2 = reader.read_u32();
        let unk3 = reader.read_u32();

        Self {
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
            recieved_events,

            num_sent_objects,
            camera_x,
            camera_y,
            camera_z,

            packet_action_count,
            num_actions: num_actions as u8,
            actions,
            voice_data,
            spectating_human_id: spectating_human_id as u8,
            unk: unk as u16,
            unk1: unk1 as u8,
            unk2,
            unk3,
        }
    }
}
