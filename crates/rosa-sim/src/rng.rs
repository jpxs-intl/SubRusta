use std::sync::Mutex;

use rand::{Rng, SeedableRng, rngs::StdRng};

/// The game's random number source, shared like the C library's rand(): reseeded by `srand`, seeded from the clock
/// until then.
static RNG: Mutex<Option<StdRng>> = Mutex::new(None);

/// rand()'s range: 0 to 2^31 - 1, so `rand() % n` and `rand() & mask` keep their meaning.
const RAND_MAX: u32 = 0x7fff_ffff;

/// Reseeds the random number source.
pub fn srand(seed: u32) {
    *RNG.lock().unwrap() = Some(StdRng::seed_from_u64(seed as u64));
}

/// A random number from 0 to 2^31 - 1.
pub fn rand() -> u32 {
    let mut guard = RNG.lock().unwrap();
    let rng = guard.get_or_insert_with(|| StdRng::seed_from_u64(time_seed() as u64));
    rng.next_u32() & RAND_MAX
}

/// A random float in [0, 1) from the top 24 bits of rand().
pub fn random_unit() -> f32 {
    (rand() >> 7) as f32 / (1u32 << 24) as f32
}

/// The seed reset_game uses (the current time).
pub fn time_seed() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as u32)
}
