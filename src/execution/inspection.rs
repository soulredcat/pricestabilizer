//! After each snapshot: mark the books and reconcile the ledger with the
//! venue's wallets.

use crate::execution::band_funding::Amounts;
use crate::execution::order_runner::OrderRunner;
use crate::execution::outcome::ExecutionOutcome;
use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::pnl_tracker::{BooksSnapshot, PnlTracker};
use crate::venue::order::Wallet;

pub struct Inspection {
    pub books: BooksSnapshot,
    pub band_held: bool,
    /// The token's share of operational value, wallet and band together.
    pub token_share: Option<f64>,
    pub reconciled: bool,
    pub loss_breached: bool,
}

/// Values everything at `valuation_price` (the TWAP): a single manipulated
/// spot print can then neither trip nor hide a loss limit. `None` when the
/// venue could not be read; the failure is already recorded.
pub fn inspect(
    runner: &mut OrderRunner,
    pnl: &mut PnlTracker,
    market: &MarketSnapshot,
    valuation_price: f64,
    initial: Amounts,
    outcome: &mut ExecutionOutcome,
) -> Option<Inspection> {
    let operational = runner.balances(Wallet::Operational, outcome)?;
    let treasury = runner.balances(Wallet::Treasury, outcome)?;
    let position = runner.position(market, outcome).ok()?;
    let (band_value, unclaimed) = position.map_or((0.0, 0.0), |position| {
        (
            position.token * valuation_price + position.quote,
            position.unclaimed_fees,
        )
    });
    let operational_amounts = Amounts::from_holding(operational, runner.scale());
    let (band_token, band_quote) =
        position.map_or((0.0, 0.0), |position| (position.token, position.quote));
    let token_value = (operational_amounts.token + band_token) * valuation_price;
    let operational_value = token_value + operational_amounts.quote + band_quote;
    let token_share = (operational_value > 0.0).then(|| token_value / operational_value);
    let wallets = Amounts::from_holding(operational.plus(treasury), runner.scale());
    let equity = wallets.value(valuation_price) + band_value + unclaimed;
    let loss_breached = pnl.mark(market.timestamp, equity);
    Some(Inspection {
        books: pnl.snapshot(
            equity,
            initial.value(valuation_price),
            unclaimed,
            runner.totals(),
        ),
        band_held: position.is_some(),
        token_share,
        reconciled: runner.reconciled(operational, treasury),
        loss_breached,
    })
}
