//! Verdict on whether an observed price may drive automatic actions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceIssue {
    /// The TWAP window has not been filled yet.
    WarmingUp,
    /// The feed went silent for too long; the TWAP restarted.
    SampleGap,
    /// Too little liquidity near spot: the price is cheap to push.
    ThinDepth,
    /// Too little volume inside the TWAP window.
    LowVolume,
    /// Spot has run away from the TWAP.
    SpotTwapDivergence,
}

impl PriceIssue {
    pub fn label(self) -> &'static str {
        match self {
            Self::WarmingUp => "warming_up",
            Self::SampleGap => "sample_gap",
            Self::ThinDepth => "thin_depth",
            Self::LowVolume => "low_volume",
            Self::SpotTwapDivergence => "spot_twap_divergence",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PriceAssessment {
    pub twap: f64,
    pub issues: Vec<PriceIssue>,
}

impl PriceAssessment {
    pub fn is_trusted(&self) -> bool {
        self.issues.is_empty()
    }

    /// The TWAP when the data is trustworthy. `None` means no reference
    /// exists right now, and the engine must not invent one.
    pub fn reference_price(&self) -> Option<f64> {
        self.is_trusted().then_some(self.twap)
    }
}
