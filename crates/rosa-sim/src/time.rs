//! Real time in server ticks, and the world clock.

/// The server runs 60 ticks a second.
pub const TICKS_PER_SECOND: i32 = 60;
pub const TICKS_PER_MINUTE: i32 = 60 * TICKS_PER_SECOND;

/// The world clock counts sixtieths of a world second.
pub const WORLD_MINUTE: i32 = 3600;
pub const WORLD_HOUR: i32 = 60 * WORLD_MINUTE;
