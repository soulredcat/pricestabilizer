//! Named scenarios covering the market conditions the strategy must survive.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioKind {
    SteadyBull,
    SteadyBear,
    Sideways,
    FlashCrash,
    PumpRetrace,
    LowLiquidity,
    LiquidityExit,
    LiquidityInflow,
    StableDepeg,
    DataGap,
}

impl ScenarioKind {
    /// Same spelling as in the config file.
    pub fn name(self) -> &'static str {
        match self {
            Self::SteadyBull => "steady_bull",
            Self::SteadyBear => "steady_bear",
            Self::Sideways => "sideways",
            Self::FlashCrash => "flash_crash",
            Self::PumpRetrace => "pump_retrace",
            Self::LowLiquidity => "low_liquidity",
            Self::LiquidityExit => "liquidity_exit",
            Self::LiquidityInflow => "liquidity_inflow",
            Self::StableDepeg => "stable_depeg",
            Self::DataGap => "data_gap",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ScenarioKind;

    const ALL: [ScenarioKind; 10] = [
        ScenarioKind::SteadyBull,
        ScenarioKind::SteadyBear,
        ScenarioKind::Sideways,
        ScenarioKind::FlashCrash,
        ScenarioKind::PumpRetrace,
        ScenarioKind::LowLiquidity,
        ScenarioKind::LiquidityExit,
        ScenarioKind::LiquidityInflow,
        ScenarioKind::StableDepeg,
        ScenarioKind::DataGap,
    ];

    #[test]
    fn names_match_the_config_spelling() {
        for kind in ALL {
            let parsed: ScenarioKind =
                serde_json::from_str(&format!("\"{}\"", kind.name())).expect("name parses");
            assert_eq!(parsed, kind);
        }
    }
}
