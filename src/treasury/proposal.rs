//! A proposed treasury purchase and the events in its life.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: u64,
    pub created_at: u64,
    /// Quote the purchase would spend.
    pub quote: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TreasuryEvent {
    Proposed(u64),
    /// One slice of an approved proposal was bought.
    Bought {
        id: u64,
        quote: f64,
    },
    /// The whole proposal has been bought.
    Completed(u64),
    /// A slice was blocked by the risk gate or failed; the proposal closes.
    NotFilled(u64),
    Rejected(u64),
    Expired(u64),
    /// An approve or reject named a proposal that is not pending.
    UnknownProposal(u64),
}

impl TreasuryEvent {
    pub fn label(self) -> &'static str {
        match self {
            Self::Proposed(_) => "treasury_proposed",
            Self::Bought { .. } => "treasury_bought",
            Self::Completed(_) => "treasury_completed",
            Self::NotFilled(_) => "treasury_not_filled",
            Self::Rejected(_) => "treasury_rejected",
            Self::Expired(_) => "treasury_expired",
            Self::UnknownProposal(_) => "treasury_unknown_proposal",
        }
    }

    /// The proposal this event is about.
    pub fn proposal_id(self) -> u64 {
        match self {
            Self::Proposed(id)
            | Self::Bought { id, .. }
            | Self::Completed(id)
            | Self::NotFilled(id)
            | Self::Rejected(id)
            | Self::Expired(id)
            | Self::UnknownProposal(id) => id,
        }
    }
}
