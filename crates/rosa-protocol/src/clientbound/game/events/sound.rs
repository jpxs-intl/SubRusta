use rosa_math::vector::Vector;

use crate::codec::WireWrite;

/// The sounds the server asks clients to play, numbered by the client's sound table (setup_game fills the server's
/// copy of those ids; the client loads the files in its sound loader).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Sound {
    /// carcrash02.wav, a car hitting something.
    CarCrash = 20,
    /// bullethitbody02.wav, a car hitting a human.
    BodyHit = 22,
    /// phonering01.wav, a phone ringing (and the ringback on the calling phone).
    PhoneRing = 27,
    /// The generated keypad tones for 0 to 9 follow this one.
    PhoneKey0 = 28,
    PhoneKey1 = 29,
    PhoneKey2 = 30,
    PhoneKey3 = 31,
    PhoneKey4 = 32,
    PhoneKey5 = 33,
    PhoneKey6 = 34,
    PhoneKey7 = 35,
    PhoneKey8 = 36,
    PhoneKey9 = 37,
    /// The generated busy tone.
    PhoneBusy = 38,
    /// reload01.wav, loading a magazine.
    Reload = 39,
    /// gearshift01.wav
    GearShift = 41,
    /// modem.wav, a computer dialling in.
    Modem = 49,
    /// floppy.wav, a computer's disk drive.
    Floppy = 50,
}

impl Sound {
    pub const PHONE_KEYS: [Sound; 10] = [
        Sound::PhoneKey0,
        Sound::PhoneKey1,
        Sound::PhoneKey2,
        Sound::PhoneKey3,
        Sound::PhoneKey4,
        Sound::PhoneKey5,
        Sound::PhoneKey6,
        Sound::PhoneKey7,
        Sound::PhoneKey8,
        Sound::PhoneKey9,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventSound {
    pub sound_type: Sound,
    pub pos: Vector,
    pub volume: f32,
    pub pitch: f32
}

impl WireWrite for EventSound {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.bits(self.sound_type as i32, 8);
        self.pos.write(w);
        w.f32(self.volume);
        w.f32(self.pitch);
    }
}