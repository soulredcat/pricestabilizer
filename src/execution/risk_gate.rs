//! Hard limits every order must pass before it reaches a venue.

use crate::config::execution_sections::RiskConfig;
use crate::market::impact::estimated_impact;
use crate::portfolio::asset::{Asset, UnitScale};
use crate::portfolio::holding::Holding;
use crate::venue::order::{VenueOrder, Wallet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskBreach {
    SwapTooLarge,
    SlippageTooHigh,
    ReserveTooLow,
    TokenExposureTooHigh,
}

impl RiskBreach {
    pub fn label(self) -> &'static str {
        match self {
            Self::SwapTooLarge => "blocked_swap_too_large",
            Self::SlippageTooHigh => "blocked_slippage_too_high",
            Self::ReserveTooLow => "blocked_reserve_too_low",
            Self::TokenExposureTooHigh => "blocked_token_exposure_too_high",
        }
    }
}

/// The state an order is judged against.
pub struct OrderContext {
    pub price: f64,
    pub quote_depth: f64,
    pub operational: Holding,
    /// Token held outside the operational wallet (band and treasury).
    pub token_elsewhere: f64,
}

pub struct RiskGate {
    limits: RiskConfig,
    scale: UnitScale,
    depth_reference_move: f64,
}

impl RiskGate {
    pub fn new(limits: &RiskConfig, scale: UnitScale, depth_reference_move: f64) -> Self {
        Self {
            limits: limits.clone(),
            scale,
            depth_reference_move,
        }
    }

    pub fn check(&self, order: &VenueOrder, context: &OrderContext) -> Result<(), RiskBreach> {
        match *order {
            VenueOrder::Swap {
                wallet,
                sell,
                units,
            } => self.check_swap(wallet, sell, units, context),
            VenueOrder::AddBand { quote_units, .. } => {
                self.check_reserve(context.operational.quote.saturating_sub(quote_units))
            }
            VenueOrder::RemoveBand => Ok(()),
        }
    }

    fn check_swap(
        &self,
        wallet: Wallet,
        sell: Asset,
        units: u128,
        context: &OrderContext,
    ) -> Result<(), RiskBreach> {
        let amount = self.scale.to_amount(sell, units);
        let notional = match sell {
            Asset::Token => amount * context.price,
            Asset::Quote => amount,
        };
        if notional > self.limits.max_swap_quote {
            return Err(RiskBreach::SwapTooLarge);
        }
        let impact = estimated_impact(notional, context.quote_depth, self.depth_reference_move);
        if impact > self.limits.max_slippage {
            return Err(RiskBreach::SlippageTooHigh);
        }
        if sell == Asset::Quote {
            if wallet == Wallet::Operational {
                self.check_reserve(context.operational.quote.saturating_sub(units))?;
            }
            let token_after = self
                .scale
                .to_amount(Asset::Token, context.operational.token)
                + context.token_elsewhere
                + notional / context.price;
            if token_after * context.price > self.limits.max_token_exposure_quote {
                return Err(RiskBreach::TokenExposureTooHigh);
            }
        }
        Ok(())
    }

    fn check_reserve(&self, quote_left: u128) -> Result<(), RiskBreach> {
        if self.scale.to_amount(Asset::Quote, quote_left) < self.limits.min_quote_reserve {
            Err(RiskBreach::ReserveTooLow)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{OrderContext, RiskBreach, RiskGate};
    use crate::config::execution_sections::RiskConfig;
    use crate::portfolio::asset::{Asset, UnitScale};
    use crate::portfolio::holding::Holding;
    use crate::venue::order::{VenueOrder, Wallet};

    fn gate() -> RiskGate {
        let limits = RiskConfig {
            max_swap_quote: 1_000.0,
            max_slippage: 0.01,
            min_quote_reserve: 500.0,
            max_token_exposure_quote: 3_000.0,
            max_daily_loss_quote: 1.0,
            max_drawdown: 0.1,
            max_consecutive_failures: 3,
        };
        RiskGate::new(&limits, UnitScale::new(0, 0), 0.02)
    }

    fn context() -> OrderContext {
        OrderContext {
            price: 1.0,
            quote_depth: 50_000.0,
            operational: Holding {
                token: 1_000,
                quote: 2_000,
            },
            token_elsewhere: 1_000.0,
        }
    }

    fn buy_with_quote(units: u128) -> VenueOrder {
        VenueOrder::Swap {
            wallet: Wallet::Operational,
            sell: Asset::Quote,
            units,
        }
    }

    #[test]
    fn passes_an_order_within_every_limit() {
        assert_eq!(gate().check(&buy_with_quote(500), &context()), Ok(()));
    }

    #[test]
    fn blocks_each_limit() {
        let gate = gate();
        assert_eq!(
            gate.check(&buy_with_quote(1_200), &context()),
            Err(RiskBreach::SwapTooLarge)
        );
        let thin = OrderContext {
            quote_depth: 800.0,
            ..context()
        };
        assert_eq!(
            gate.check(&buy_with_quote(900), &thin),
            Err(RiskBreach::SlippageTooHigh)
        );
        let low = OrderContext {
            operational: Holding {
                token: 0,
                quote: 1_000,
            },
            ..context()
        };
        assert_eq!(
            gate.check(&buy_with_quote(600), &low),
            Err(RiskBreach::ReserveTooLow)
        );
        let heavy = OrderContext {
            token_elsewhere: 1_600.0,
            ..context()
        };
        assert_eq!(
            gate.check(&buy_with_quote(500), &heavy),
            Err(RiskBreach::TokenExposureTooHigh)
        );
    }
}
