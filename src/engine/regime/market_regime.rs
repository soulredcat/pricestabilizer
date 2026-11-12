//! Regime vocabulary shared by the classifier, mode controller and report.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Regime {
    Bullish,
    Sideways,
    Bearish,
    HighVolatility,
    /// Price data is not trustworthy enough to act on.
    LowConfidence,
    /// Something is abnormal enough to trip protection.
    Emergency(EmergencyCause),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmergencyCause {
    StableDepeg,
    ExtremeMove,
    DepthCollapse,
    /// Daily loss or drawdown limit breached.
    LossLimit,
    /// The ledger no longer matches what the venue reports.
    LedgerMismatch,
    /// Too many venue orders failed in a row.
    VenueFailures,
    /// A venue reported a fill that does not match its order.
    ExecutionAnomaly,
    /// The daily order limit was hit: possibly a runaway loop.
    OrderBurst,
    /// An operator paused the engine.
    OperatorPause,
}

impl Regime {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bullish => "bullish",
            Self::Sideways => "sideways",
            Self::Bearish => "bearish",
            Self::HighVolatility => "high_volatility",
            Self::LowConfidence => "low_confidence",
            Self::Emergency(_) => "emergency",
        }
    }
}

impl EmergencyCause {
    pub fn label(self) -> &'static str {
        match self {
            Self::StableDepeg => "stable_depeg",
            Self::ExtremeMove => "extreme_move",
            Self::DepthCollapse => "depth_collapse",
            Self::LossLimit => "loss_limit",
            Self::LedgerMismatch => "ledger_mismatch",
            Self::VenueFailures => "venue_failures",
            Self::ExecutionAnomaly => "execution_anomaly",
            Self::OrderBurst => "order_burst",
            Self::OperatorPause => "operator_pause",
        }
    }
}
