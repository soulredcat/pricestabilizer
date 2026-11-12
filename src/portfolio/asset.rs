//! The two assets of the pair and their integer base units.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Asset {
    Token,
    Quote,
}

impl Asset {
    pub fn counterpart(self) -> Self {
        match self {
            Self::Token => Self::Quote,
            Self::Quote => Self::Token,
        }
    }
}

/// Converts human amounts to integer base units and back.
#[derive(Debug, Clone, Copy)]
pub struct UnitScale {
    token_factor: f64,
    quote_factor: f64,
}

impl UnitScale {
    pub fn new(token_decimals: u32, quote_decimals: u32) -> Self {
        Self {
            token_factor: 10_f64.powi(token_decimals as i32),
            quote_factor: 10_f64.powi(quote_decimals as i32),
        }
    }

    /// Rounds down, so the engine never books more than it holds.
    pub fn to_units(self, asset: Asset, amount: f64) -> u128 {
        (amount.max(0.0) * self.factor(asset)).floor() as u128
    }

    pub fn to_amount(self, asset: Asset, units: u128) -> f64 {
        units as f64 / self.factor(asset)
    }

    fn factor(self, asset: Asset) -> f64 {
        match asset {
            Asset::Token => self.token_factor,
            Asset::Quote => self.quote_factor,
        }
    }
}

/// Writes base units as a decimal string: JSON numbers lose precision past
/// 2^53, which 18-decimal assets exceed quickly.
pub fn serialize_units<S: Serializer>(units: &u128, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(units)
}

/// Signed balances (the capital account goes negative), as a string too.
pub fn serialize_signed_units<S: Serializer>(
    units: &i128,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_str(units)
}

pub fn deserialize_signed_units<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<i128, D::Error> {
    let text = String::deserialize(deserializer)?;
    text.parse().map_err(serde::de::Error::custom)
}

/// Reads base units written by [`serialize_units`]: a decimal string.
pub fn deserialize_units<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u128, D::Error> {
    let text = String::deserialize(deserializer)?;
    text.parse().map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::{Asset, UnitScale};

    #[test]
    fn round_trips_with_each_assets_decimals() {
        let scale = UnitScale::new(18, 6);
        assert_eq!(scale.to_units(Asset::Quote, 12.345_678_9), 12_345_678);
        assert_eq!(scale.to_units(Asset::Token, 1.5), 1_500_000_000_000_000_000);
        assert!((scale.to_amount(Asset::Quote, 2_500_000) - 2.5).abs() < 1e-12);
    }

    #[test]
    fn negative_amounts_become_zero_units() {
        assert_eq!(UnitScale::new(6, 6).to_units(Asset::Token, -3.0), 0);
    }
}
