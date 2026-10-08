use std::hash::{BuildHasher, Hasher};

pub fn rand() -> u32 {
    let bits = std::collections::hash_map::RandomState::new().build_hasher().finish();
    (bits >> 33) as u32
}

pub fn random_unit() -> f32 {
    (rand() >> 7) as f32 / (1u32 << 24) as f32
}
