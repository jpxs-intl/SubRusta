use std::sync::RwLock;

use dashmap::DashMap;

use crate::connection::packets::serverbound::game::opus::ServerboundGameVoiceFrame;

#[derive(Default)]
pub struct VoiceManager {
    pub voice_enabled: RwLock<bool>,
    pub client_voices: DashMap<u32, PlayerVoice>
}

pub struct PlayerVoice {
    pub enabled: bool,
    pub client_id: u32,
    pub frames: Vec<ServerboundGameVoiceFrame>
}

impl VoiceManager {
    pub fn new() -> Self {
        VoiceManager {
            voice_enabled: RwLock::new(true),
            client_voices: DashMap::new()
        }
    }
}