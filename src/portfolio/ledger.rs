//! Double-entry ledger in integer base units. Every movement names where the
//! units came from and where they went, so balances can always be rebuilt
//! from the entries and compared with what the venue reports.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::portfolio::asset::{
    Asset, deserialize_signed_units, serialize_signed_units, serialize_units,
};
use crate::portfolio::holding::Holding;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Account {
    /// Outside capital that funded the wallets.
    Capital,
    /// Free inventory in the operational wallet.
    Operational,
    /// What was deposited into the liquidity band.
    Band,
    /// Treasury wallet, kept apart from operations.
    Treasury,
    /// Counterparty for swaps, band conversions and fees.
    Market,
    /// Network and order costs paid.
    Costs,
}

impl Account {
    /// Accounts holding real assets can never go below zero.
    fn holds_assets(self) -> bool {
        matches!(self, Self::Operational | Self::Band | Self::Treasury)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Deposit,
    Swap,
    AddLiquidity,
    RemoveLiquidity,
    FeeClaim,
    ActionCost,
    TreasuryBuy,
    /// An operator's reconciliation with the venue after transfers made
    /// outside the engine.
    Adjustment,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Entry {
    pub timestamp: u64,
    pub kind: EntryKind,
    pub asset: Asset,
    pub from: Account,
    pub to: Account,
    #[serde(serialize_with = "serialize_units")]
    pub units: u128,
}

/// One ledger balance, saved so the books survive a restart.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BalanceRow {
    pub account: Account,
    pub asset: Asset,
    #[serde(
        serialize_with = "serialize_signed_units",
        deserialize_with = "deserialize_signed_units"
    )]
    pub units: i128,
}

/// A posting would have taken a holding account below zero.
#[derive(Debug)]
pub struct Overdraft;

#[derive(Debug, Default)]
pub struct Ledger {
    entries: Vec<Entry>,
    balances: BTreeMap<(Account, Asset), i128>,
}

impl Ledger {
    /// Capital entering a wallet; the capital account may go negative.
    pub fn deposit(&mut self, timestamp: u64, to: Account, holding: Holding) {
        for asset in [Asset::Token, Asset::Quote] {
            self.apply(Entry {
                timestamp,
                kind: EntryKind::Deposit,
                asset,
                from: Account::Capital,
                to,
                units: holding.get(asset),
            });
        }
    }

    /// Balances only: the entries themselves live in the journal.
    pub fn balances(&self) -> Vec<BalanceRow> {
        self.balances
            .iter()
            .map(|(&(account, asset), &units)| BalanceRow {
                account,
                asset,
                units,
            })
            .collect()
    }

    pub fn from_balances(rows: &[BalanceRow]) -> Self {
        Self {
            entries: Vec::new(),
            balances: rows
                .iter()
                .map(|row| ((row.account, row.asset), row.units))
                .collect(),
        }
    }

    /// Books whatever makes `account` hold exactly `target`, against the
    /// capital account: the operator's reconciliation with the venue.
    pub fn adjust_to(&mut self, timestamp: u64, account: Account, target: Holding) {
        for asset in [Asset::Token, Asset::Quote] {
            let current = self.balance(account, asset);
            let wanted = signed(target.get(asset));
            let (from, to) = if wanted > current {
                (Account::Capital, account)
            } else {
                (account, Account::Capital)
            };
            self.apply(Entry {
                timestamp,
                kind: EntryKind::Adjustment,
                asset,
                from,
                to,
                units: wanted.abs_diff(current),
            });
        }
    }

    pub fn post(&mut self, entry: Entry) -> Result<(), Overdraft> {
        let after = self.balance(entry.from, entry.asset) - signed(entry.units);
        if entry.from.holds_assets() && after < 0 {
            return Err(Overdraft);
        }
        self.apply(entry);
        Ok(())
    }

    pub fn holding(&self, account: Account) -> Holding {
        let units = |asset| u128::try_from(self.balance(account, asset)).unwrap_or(0);
        Holding {
            token: units(Asset::Token),
            quote: units(Asset::Quote),
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    fn balance(&self, account: Account, asset: Asset) -> i128 {
        self.balances.get(&(account, asset)).copied().unwrap_or(0)
    }

    fn apply(&mut self, entry: Entry) {
        if entry.units == 0 {
            return;
        }
        let amount = signed(entry.units);
        *self.balances.entry((entry.from, entry.asset)).or_insert(0) -= amount;
        *self.balances.entry((entry.to, entry.asset)).or_insert(0) += amount;
        self.entries.push(entry);
    }
}

fn signed(units: u128) -> i128 {
    i128::try_from(units).unwrap_or(i128::MAX)
}

#[cfg(test)]
mod tests {
    use super::{Account, Entry, EntryKind, Ledger};
    use crate::portfolio::asset::Asset;
    use crate::portfolio::holding::Holding;

    fn swap_out(units: u128) -> Entry {
        Entry {
            timestamp: 1,
            kind: EntryKind::Swap,
            asset: Asset::Quote,
            from: Account::Operational,
            to: Account::Market,
            units,
        }
    }

    #[test]
    fn deposits_fund_wallets_and_postings_move_them() {
        let mut ledger = Ledger::default();
        ledger.deposit(
            0,
            Account::Operational,
            Holding {
                token: 7,
                quote: 10,
            },
        );
        ledger.post(swap_out(4)).expect("funded");
        assert_eq!(
            ledger.holding(Account::Operational),
            Holding { token: 7, quote: 6 }
        );
        assert_eq!(ledger.entries().len(), 3);
    }

    #[test]
    fn balances_round_trip_and_adjustments_reach_the_target() {
        let mut ledger = Ledger::default();
        ledger.deposit(
            0,
            Account::Operational,
            Holding {
                token: 7,
                quote: 10,
            },
        );
        let mut restored = Ledger::from_balances(&ledger.balances());
        assert_eq!(
            restored.holding(Account::Operational),
            Holding {
                token: 7,
                quote: 10
            }
        );
        restored.adjust_to(5, Account::Operational, Holding { token: 9, quote: 4 });
        assert_eq!(
            restored.holding(Account::Operational),
            Holding { token: 9, quote: 4 }
        );
        assert_eq!(restored.entries().len(), 2);
    }

    #[test]
    fn overdrafts_are_refused_and_not_recorded() {
        let mut ledger = Ledger::default();
        ledger.deposit(0, Account::Operational, Holding { token: 0, quote: 3 });
        assert!(ledger.post(swap_out(4)).is_err());
        assert_eq!(ledger.holding(Account::Operational).quote, 3);
    }
}
