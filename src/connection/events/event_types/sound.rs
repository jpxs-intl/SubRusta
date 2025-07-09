use crate::{connection::packets::{buf_writer::AlexBufWriter, WriterEncodable}, world::vector::Vector};

#[derive(Clone, Copy)]
pub enum SoundType {
    CarEngine = 8,
    TireDrift = 10,
    Ricochet = 11, // 11-18
    CarCrash1 = 19,
    CarCrash2 = 20,
    BulletHitBody1 = 21,
    BulletHitBody2 = 22,
    BulletHitMetal1 = 23,
    BulletHitMetal2 = 24,
    GlassBreak = 25,
    PhoneRing = 27,
    PhoneButton0 = 28,
    PhoneButton1 = 29,
    PhoneButton2 = 30,
    PhoneButton3 = 31,
    PhoneButton4 = 32,
    PhoneButton5 = 33,
    PhoneButton6 = 34,
    PhoneButton7 = 35,
    PhoneButton8 = 36,
    PhoneButton9 = 37,
    PhoneBusy = 38,
    MagazineLoad = 39,
    BulletShellBounce = 40,
    GearShift = 41,
    Helicopter = 42,
    Train1 = 43,
    Train2 = 44,
    Train3 = 45,
    Train4 = 46,
    FactoryWhistle = 47,
    Explosion = 48,
    ComputerDialup = 49,
    ComputerDrive = 50,
    Ak47Fire1 = 71,
    M16Fire1 = 83,
    UziFire1 = 89,
    NineMMFire1 = 95
}

#[derive(Clone, Copy)]
pub struct EventSound {
    pub tick_created: i32,
    pub sound_type: SoundType,
    pub pos: Vector,
    pub volume: f32,
    pub pitch: f32
}

impl WriterEncodable for EventSound {
    fn encode(&self, _state: &crate::AppState, writer: &mut AlexBufWriter) {
        writer.write_bits(9, 6);
        writer.write_bits(self.tick_created, 28);

        writer.write_bits(self.sound_type as i32, 8);
        self.pos.encode(writer);
        writer.write_bytes(&self.volume.to_le_bytes());
        writer.write_bytes(&self.pitch.to_le_bytes());
    }
}