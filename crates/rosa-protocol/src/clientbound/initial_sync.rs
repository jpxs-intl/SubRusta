use crate::{GameMode, codec::WireWrite};

#[derive(Debug, Clone, PartialEq)]
pub struct InitialSync {
    pub round_number: u32,
    pub gamemode: GameMode,
    pub map_name: String,
    pub weekly_enabled: bool,
    pub weekday: u8,
    pub sun_angle: u16,
    pub sun_axial_tilt: u16,
    pub versus_movedelay: Option<u8>,
}

impl WireWrite for InitialSync {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.byte(0x06);
        w.u32(self.round_number);
        w.bits(self.gamemode as i32, 4);
        w.bits(self.weekly_enabled as i32, 4);
        w.byte(self.weekday);
        w.string(&self.map_name, 32);
        w.u16(self.sun_angle);
        w.u16(self.sun_axial_tilt);

        if self.gamemode == GameMode::Versus {
            w.byte(self.versus_movedelay.unwrap_or(0))
        }
    }
}