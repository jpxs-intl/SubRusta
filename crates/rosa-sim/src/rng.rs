use std::sync::Mutex;

/// glibc's rand() (random_r with the default TYPE_3 additive feedback generator): 34 seed words from a Lehmer
/// generator, 310 discarded outputs, then each output the sum of the words 31 and 3 back, halved.
struct Glibc {
    r: [i32; 34],
    i: usize,
}

const SEPARATION: usize = 31;
const SHORT_TAP: usize = 3;
const DISCARD: usize = 310;

impl Glibc {
    const fn unseeded() -> Self {
        Glibc { r: [0; 34], i: 0 }
    }

    fn seed(&mut self, seed: u32) {
        let mut r = [0i32; 34];
        r[0] = if seed == 0 { 1 } else { seed as i32 };
        for k in 1..SEPARATION {
            let hi = r[k - 1] / 127773;
            let lo = r[k - 1] % 127773;
            let mut word = 16807 * lo - 2836 * hi;
            if word < 0 {
                word += 2147483647;
            }
            r[k] = word;
        }
        for k in SEPARATION..34 {
            r[k] = r[k - SEPARATION];
        }
        self.r = r;
        self.i = 0;
        for _ in 0..DISCARD {
            self.next();
        }
    }

    fn next(&mut self) -> u32 {
        let n = self.r.len();
        let word = self.r[(self.i + n - SEPARATION) % n].wrapping_add(self.r[(self.i + n - SHORT_TAP) % n]);
        self.r[self.i] = word;
        self.i = (self.i + 1) % n;
        (word as u32) >> 1
    }
}

static RNG: Mutex<Option<Glibc>> = Mutex::new(None);

/// srand(seed).
pub fn srand(seed: u32) {
    let mut g = Glibc::unseeded();
    g.seed(seed);
    *RNG.lock().unwrap() = Some(g);
}

/// rand(): 0..=2^31-1, seeded with 1 until srand is called, as glibc is.
pub fn rand() -> u32 {
    let mut guard = RNG.lock().unwrap();
    let g = guard.get_or_insert_with(|| {
        let mut g = Glibc::unseeded();
        g.seed(1);
        g
    });
    g.next()
}

/// A random float in [0, 1) from the top 24 bits of rand().
pub fn random_unit() -> f32 {
    (rand() >> 7) as f32 / (1u32 << 24) as f32
}

/// The seed reset_game uses (time(NULL)).
pub fn time_seed() -> u32 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as u32)
}

#[cfg(test)]
mod tests {
    #[test]
    fn matches_glibc() {
        super::srand(1);
        let first: Vec<u32> = (0..5).map(|_| super::rand()).collect();
        assert_eq!(first, [1804289383, 846930886, 1681692777, 1714636915, 1957747793]);
    }
}
