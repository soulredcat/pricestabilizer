//! Validates a whole configuration, section by section.

use crate::app::error::AppError;
use crate::config::engine_config::EngineConfig;
use crate::config::validation::engine_rules::{
    check_growth, check_liquidity, check_price, check_regime,
};
use crate::config::validation::execution_rules::{
    check_assets, check_feed, check_risk, check_treasury, check_venue,
};

impl EngineConfig {
    pub fn validate(&self) -> Result<(), AppError> {
        check_growth(&self.growth)?;
        check_price(&self.price)?;
        check_regime(&self.regime)?;
        check_liquidity(&self.liquidity)?;
        check_assets(&self.assets)?;
        check_risk(&self.risk)?;
        check_venue(&self.venue)?;
        check_treasury(&self.treasury)?;
        check_feed(&self.feed)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::app::error::AppError;
    use crate::config::engine_config::EngineConfig;

    const EXAMPLE: &str = include_str!("../../../config/simulation.example.toml");

    #[test]
    fn example_config_is_valid() {
        assert!(EngineConfig::parse(EXAMPLE, Path::new("example")).is_ok());
    }

    #[test]
    fn inverted_range_bounds_are_rejected() {
        let broken = EXAMPLE.replace("min_half_width = 0.02", "min_half_width = 0.9");
        let error = EngineConfig::parse(&broken, Path::new("broken")).unwrap_err();
        assert!(matches!(
            error,
            AppError::ConfigInvalid {
                field: "liquidity.min_half_width",
                ..
            }
        ));
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let typo = EXAMPLE.replace("weekly_growth", "weekly_grow");
        let error = EngineConfig::parse(&typo, Path::new("typo")).unwrap_err();
        assert!(matches!(error, AppError::ConfigParse { .. }));
    }

    #[test]
    fn missing_risk_limits_are_rejected() {
        let missing = EXAMPLE.replace("max_drawdown = 0.15\n", "");
        let error = EngineConfig::parse(&missing, Path::new("missing")).unwrap_err();
        assert!(matches!(error, AppError::ConfigParse { .. }));
    }
}
