//! A small random generator that gives the same stream on every platform, so that a search run
//! can be repeated from its seed.

/// xoshiro256** (Blackman and Vigna), seeded through SplitMix64.
#[derive(Clone, Debug)]
pub struct Rng([u64; 4]);

impl Rng {
    /// A generator whose whole stream is fixed by `seed`.
    pub fn new(seed: u64) -> Self {
        let mut state = seed;
        let mut next = || {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Self([next(), next(), next(), next()])
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.0;
        let out = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let shifted = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= shifted;
        s[3] = s[3].rotate_left(45);
        out
    }

    /// A number below `n` (`n > 0`). The bias of the plain remainder is far below anything a
    /// search heuristic can notice.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn the_stream_is_fixed() {
        // Reference values of xoshiro256** after SplitMix64 seeding with 0.
        let mut rng = Rng::new(0);
        assert_eq!(rng.next_u64(), 0x99EC_5F36_CB75_F2B4);
        assert_eq!(rng.next_u64(), 0xBF6E_1F78_4956_452A);
        assert_eq!(rng.next_u64(), 0x1A5F_849D_4933_E6E0);
    }

    #[test]
    fn below_stays_below() {
        let mut rng = Rng::new(7);
        assert!((0..1000).all(|_| rng.below(13) < 13));
    }
}
