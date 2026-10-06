use rosa_protocol::serverbound::game::voice::{VoiceData, VoiceFrame};

pub struct PlayerVoice {
    pub frames: [Option<VoiceFrame>; 64],
    pub current: u8,
    pub is_silenced: bool
}

impl Default for PlayerVoice {
    fn default() -> Self {
        Self { frames: [const { None }; 64], current: Default::default(), is_silenced: Default::default() }
    }
}

impl PlayerVoice {
    pub fn new() -> Self {
        Self {
            frames: [const { None }; 64],
            current: 0,
            is_silenced: true
        }
    }

    pub fn ingest(&mut self, voice_data: VoiceData) {
        self.is_silenced = voice_data.is_silenced;

        for frame in voice_data.frames {
            self.push(frame.index, frame);
        }
    }

    pub fn push(&mut self, index: u8, frame: VoiceFrame) {
        let slot = (index % 64) as usize;
        self.frames[slot] = Some(frame);
        self.current = index.wrapping_add(1) % 64;
    }

    pub fn recent(&self, n: u8) -> impl Iterator<Item = &VoiceFrame> + '_ {
        let start = self.current.wrapping_sub(n) % 64;
        (0..n).filter_map(move |i| {
            let idx = start.wrapping_add(i) % 64;
            self.frames[idx as usize].as_ref()
        })
    }

    pub fn recent4(&self) -> [VoiceFrame; 4] {
        let start = self.current.wrapping_sub(4) % 64;
        std::array::from_fn(|i| {
            let idx = start.wrapping_add(i as u8) % 64;
            self.frames[idx as usize].clone().unwrap_or_else(|| VoiceFrame {
                index: idx,
                size: 0,
                volume: 0,
                data: Vec::new()
            })
        })
    }
}
