#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum Weekday {
    Sunday = 0,
    Monday = 1,
    Tuesday = 2,
    Wednesday = 3,
    Thursday = 4,
    Friday = 5,
    Saturday = 6,
}

pub struct World {
    sun_angle: u16,
    sun_axial_tilt: u16,
    pub weekday: Weekday,
    pub map_name: String,
}

impl World {
    pub fn new(map_name: String) -> Self {
        World {
            sun_angle: 1000,
            sun_axial_tilt: 1000,
            weekday: Weekday::Monday,
            map_name
        }
    }

    pub fn sun_angle(&self) -> u16 { self.sun_angle }
    pub fn sun_axial_tilt(&self) -> u16 { self.sun_axial_tilt }
}