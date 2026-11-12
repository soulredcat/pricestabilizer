//! Small deterministic PRNG, so every scenario run is reproducible from its
//! seed without pulling in an RNG dependency.

pub struct SeededRandom {
    state: u64,
}

impl SeededRandom {
    pub fn new(seed: u64) -> Self {
        // SplitMix64 scramble: nearby seeds give unrelated streams, and `| 1`
        // keeps xorshift away from its all-zero fixed point.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Self {
            state: (z ^ (z >> 31)) | 1,
        }
    }

    /// Standard normal sample (Box–Muller).
    pub fn next_gaussian(&mut self) -> f64 {
        let radius = (-2.0 * (1.0 - self.next_unit()).ln()).sqrt();
        radius * (std::f64::consts::TAU * self.next_unit()).cos()
    }

    /// Uniform sample in `[0, 1)`.
    fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1_u64 << 53) as f64
    }

    /// xorshift64*.
    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

#[cfg(test)]
mod tests {
    use super::SeededRandom;

    #[test]
    fn same_seed_same_stream() {
        let mut first = SeededRandom::new(7);
        let mut second = SeededRandom::new(7);
        for _ in 0..100 {
            assert_eq!(
                first.next_gaussian().to_bits(),
                second.next_gaussian().to_bits()
            );
        }
    }

    #[test]
    fn gaussian_has_unit_moments() {
        let mut random = SeededRandom::new(42);
        let samples: Vec<f64> = (0..20_000).map(|_| random.next_gaussian()).collect();
        let mean = samples.iter().sum::<f64>() / samples.len() as f64;
        let variance =
            samples.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / samples.len() as f64;
        assert!(mean.abs() < 0.03, "mean {mean}");
        assert!((variance - 1.0).abs() < 0.05, "variance {variance}");
    }
}
