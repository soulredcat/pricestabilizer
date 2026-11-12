//! Regime vocabulary shared by the classifier, mode controller and report.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmergencyCause {
    StableDepeg,
    ExtremeMove,
    DepthCollapse,
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
        }
    }
}
