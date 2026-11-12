//! A price band `[lower, upper]` in quote units: prices, never ticks or levels.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PriceBand {
    pub lower: f64,
    pub upper: f64,
}

impl PriceBand {
    /// Geometric band centred on `center`, `half_width` relative (0.05 = ±5%).
    pub fn around(center: f64, half_width: f64) -> Self {
        Self {
            lower: center / (1.0 + half_width),
            upper: center * (1.0 + half_width),
        }
    }

    pub fn contains(&self, price: f64) -> bool {
        (self.lower..=self.upper).contains(&price)
    }

    pub fn half_width(&self) -> f64 {
        (self.upper / self.lower).sqrt() - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::PriceBand;

    #[test]
    fn is_symmetric_in_log_space() {
        let band = PriceBand::around(2.0, 0.1);
        assert!((band.lower * band.upper - 4.0).abs() < 1e-12);
        assert!((band.half_width() - 0.1).abs() < 1e-12);
        assert!(band.contains(2.0) && !band.contains(2.3));
    }
}
