//! Posts venue fills to the double-entry ledger.

use crate::portfolio::asset::Asset;
use crate::portfolio::ledger::{Account, Entry, EntryKind, Ledger, Overdraft};
use crate::venue::order::{Fill, VenueOrder, Wallet};

pub fn book(
    ledger: &mut Ledger,
    timestamp: u64,
    order: &VenueOrder,
    fill: &Fill,
) -> Result<(), Overdraft> {
    let mut poster = Poster { ledger, timestamp };
    let principal_quote = fill.spent.quote.saturating_sub(fill.action_cost);
    match *order {
        VenueOrder::Swap {
            wallet,
            sell,
            units,
        } => {
            let account = account_of(wallet);
            let bought = sell.counterpart();
            poster.post(EntryKind::Swap, sell, account, Account::Market, units)?;
            poster.post(
                EntryKind::Swap,
                bought,
                Account::Market,
                account,
                fill.received.get(bought),
            )?;
            poster.post(
                EntryKind::ActionCost,
                Asset::Quote,
                account,
                Account::Costs,
                fill.action_cost,
            )
        }
        VenueOrder::AddBand { .. } => {
            let (from, to) = (Account::Operational, Account::Band);
            poster.post(
                EntryKind::AddLiquidity,
                Asset::Token,
                from,
                to,
                fill.spent.token,
            )?;
            poster.post(
                EntryKind::AddLiquidity,
                Asset::Quote,
                from,
                to,
                principal_quote,
            )?;
            poster.post(
                EntryKind::ActionCost,
                Asset::Quote,
                from,
                Account::Costs,
                fill.action_cost,
            )
        }
        VenueOrder::RemoveBand => {
            // The band's deposits go back to the market; what the position is
            // worth now comes back from it. The difference is the band's PnL.
            let deposited = poster.ledger.holding(Account::Band);
            let (band, market, wallet) = (Account::Band, Account::Market, Account::Operational);
            let returned_quote = fill.received.quote.saturating_sub(fill.fees_claimed);
            poster.post(
                EntryKind::RemoveLiquidity,
                Asset::Token,
                band,
                market,
                deposited.token,
            )?;
            poster.post(
                EntryKind::RemoveLiquidity,
                Asset::Quote,
                band,
                market,
                deposited.quote,
            )?;
            poster.post(
                EntryKind::RemoveLiquidity,
                Asset::Token,
                market,
                wallet,
                fill.received.token,
            )?;
            poster.post(
                EntryKind::RemoveLiquidity,
                Asset::Quote,
                market,
                wallet,
                returned_quote,
            )?;
            poster.post(
                EntryKind::FeeClaim,
                Asset::Quote,
                market,
                wallet,
                fill.fees_claimed,
            )?;
            poster.post(
                EntryKind::ActionCost,
                Asset::Quote,
                wallet,
                Account::Costs,
                fill.action_cost,
            )
        }
    }
}

pub fn account_of(wallet: Wallet) -> Account {
    match wallet {
        Wallet::Operational => Account::Operational,
        Wallet::Treasury => Account::Treasury,
    }
}

struct Poster<'a> {
    ledger: &'a mut Ledger,
    timestamp: u64,
}

impl Poster<'_> {
    fn post(
        &mut self,
        kind: EntryKind,
        asset: Asset,
        from: Account,
        to: Account,
        units: u128,
    ) -> Result<(), Overdraft> {
        self.ledger.post(Entry {
            timestamp: self.timestamp,
            kind,
            asset,
            from,
            to,
            units,
        })
    }
}
