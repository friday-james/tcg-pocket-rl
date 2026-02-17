use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Seedable RNG wrapper for deterministic game simulation.
#[derive(Debug, Clone)]
pub struct GameRng {
    rng: StdRng,
    guaranteed_heads: bool,
}

impl GameRng {
    pub fn new(seed: u64) -> Self {
        GameRng {
            rng: StdRng::seed_from_u64(seed),
            guaranteed_heads: false,
        }
    }

    /// Flip a coin. Returns true for heads, false for tails.
    pub fn coin_flip(&mut self) -> bool {
        if self.guaranteed_heads {
            self.guaranteed_heads = false;
            return true;
        }
        self.rng.gen_bool(0.5)
    }

    /// Set guaranteed heads for next coin flip (Will supporter effect).
    pub fn set_guaranteed_heads(&mut self, value: bool) {
        self.guaranteed_heads = value;
    }

    /// Flip a coin multiple times, return number of heads.
    pub fn coin_flips(&mut self, count: u32) -> u32 {
        (0..count).filter(|_| self.coin_flip()).count() as u32
    }

    /// Shuffle a vec in place.
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        // Fisher-Yates shuffle
        let len = slice.len();
        for i in (1..len).rev() {
            let j = self.rng.gen_range(0..=i);
            slice.swap(i, j);
        }
    }

    /// Generate a random number in range [0, max).
    pub fn gen_range(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        self.rng.gen_range(min..max)
    }
}

// Serde support: serialize as seed (not full state)
impl serde::Serialize for GameRng {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // We can't easily serialize StdRng state, so just serialize a marker
        serializer.serialize_u64(0)
    }
}

impl<'de> serde::Deserialize<'de> for GameRng {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let seed = u64::deserialize(deserializer)?;
        Ok(GameRng::new(seed))
    }
}
