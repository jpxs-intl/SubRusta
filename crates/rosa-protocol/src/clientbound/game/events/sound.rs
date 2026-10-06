use rosa_math::vector::Vector;

use crate::codec::WireWrite;

#[derive(Clone, Copy, Debug, PartialEq)]
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventSound {
    pub sound_type: SoundType,
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