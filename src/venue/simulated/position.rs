//! A band position inside the paper venue.

use crate::engine::liquidity::band_math;
use crate::engine::liquidity::price_band::PriceBand;
use crate::market::snapshot::MarketSnapshot;

pub struct PaperPosition {
    pub band: PriceBand,
    pub liquidity: f64,
    pub unclaimed_fees: f64,
}

impl PaperPosition {
    /// Fees for one snapshot: the snapshot's depth counts as other providers,
    /// and our share of depth near spot earns that share of the fee on volume.
    pub fn accrue_fees(
        &mut self,
        market: &MarketSnapshot,
        fee_rate: f64,
        depth_reference_move: f64,
    ) {
        if !self.band.contains(market.spot_price) {
            return;
        }
        let ours = band_math::quote_depth(
            self.band,
            self.liquidity,
            market.spot_price,
            depth_reference_move,
        );
        let total = ours + market.quote_depth;
        if total > 0.0 {
            self.unclaimed_fees += market.quote_volume * fee_rate * ours / total;
        }
    }

    pub fn amounts(&self, price: f64) -> (f64, f64) {
        let (token, quote) = band_math::amounts_per_liquidity(self.band, price);
        (token * self.liquidity, quote * self.liquidity)
    }
}

#[cfg(test)]
mod tests {
    use super::PaperPosition;
    use crate::engine::liquidity::price_band::PriceBand;
    use crate::market::snapshot::MarketSnapshot;

    fn market(spot_price: f64) -> MarketSnapshot {
        MarketSnapshot {
            timestamp: 0,
            spot_price,
            quote_depth: 1_000.0,
            quote_volume: 500.0,
            quote_usd_price: 1.0,
        }
    }

    #[test]
    fn earns_only_while_spot_is_inside() {
        let mut position = PaperPosition {
            band: PriceBand::around(1.0, 0.1),
            liquidity: 10_000.0,
            unclaimed_fees: 0.0,
        };
        position.accrue_fees(&market(1.0), 0.003, 0.02);
        let earned = position.unclaimed_fees;
        assert!(earned > 0.0 && earned < 500.0 * 0.003);
        position.accrue_fees(&market(2.0), 0.003, 0.02);
        assert_eq!(position.unclaimed_fees, earned);
    }
}
