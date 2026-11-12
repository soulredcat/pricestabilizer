//! Base units of both assets held in one place.

use serde::{Deserialize, Serialize};

use crate::portfolio::asset::{Asset, deserialize_units, serialize_units};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holding {
    #[serde(
        serialize_with = "serialize_units",
        deserialize_with = "deserialize_units"
    )]
    pub token: u128,
    #[serde(
        serialize_with = "serialize_units",
        deserialize_with = "deserialize_units"
    )]
    pub quote: u128,
}

impl Holding {
    pub fn of(asset: Asset, units: u128) -> Self {
        match asset {
            Asset::Token => Self {
                token: units,
                quote: 0,
            },
            Asset::Quote => Self {
                token: 0,
                quote: units,
            },
        }
    }

    pub fn get(self, asset: Asset) -> u128 {
        match asset {
            Asset::Token => self.token,
            Asset::Quote => self.quote,
        }
    }

    pub fn plus(self, other: Self) -> Self {
        Self {
            token: self.token.saturating_add(other.token),
            quote: self.quote.saturating_add(other.quote),
        }
    }

    /// `None` when either asset would go below zero.
    pub fn minus(self, other: Self) -> Option<Self> {
        Some(Self {
            token: self.token.checked_sub(other.token)?,
            quote: self.quote.checked_sub(other.quote)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Holding;
    use crate::portfolio::asset::Asset;

    #[test]
    fn subtraction_refuses_to_overdraw() {
        let wallet = Holding { token: 5, quote: 5 };
        assert_eq!(
            wallet.minus(Holding::of(Asset::Quote, 2)),
            Some(Holding { token: 5, quote: 3 })
        );
        assert_eq!(wallet.minus(Holding::of(Asset::Token, 6)), None);
        assert_eq!(
            wallet.plus(Holding::of(Asset::Token, 1)).get(Asset::Token),
            6
        );
    }
}
