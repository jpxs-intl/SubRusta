use crate::{clientbound::game::events::sound::Sound, codec::WireWrite};

/// Event 0x13: a phone plays a sound (ringing, the ringback tone, a key tone).
#[derive(Clone, Debug, PartialEq)]
pub struct EventPhoneSound {
    pub sound: Sound,
    pub item_id: i32,
    pub volume: f32,
    pub pitch: f32,
}

impl WireWrite for EventPhoneSound {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.bits(self.sound as i32, 8);
        w.bits(self.item_id, 16);
        w.f32(self.volume);
        w.f32(self.pitch);
    }
}
