//! Root of the configuration file.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::app::error::AppError;
use crate::config::engine_sections::{GrowthConfig, LiquidityConfig, PriceConfig, RegimeConfig};
use crate::config::execution_sections::{AssetsConfig, RiskConfig};
use crate::config::feed_section::FeedConfig;
use crate::config::report_section::ReportConfig;
use crate::config::treasury_section::{OperatorConfig, TreasuryConfig};
use crate::config::venue_section::VenueConfig;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfig {
    pub growth: GrowthConfig,
    pub price: PriceConfig,
    pub regime: RegimeConfig,
    pub liquidity: LiquidityConfig,
    pub assets: AssetsConfig,
    pub risk: RiskConfig,
    pub venue: VenueConfig,
    pub treasury: TreasuryConfig,
    pub feed: FeedConfig,
    #[serde(default)]
    pub operator: OperatorConfig,
    #[serde(default)]
    pub report: ReportConfig,
}

impl EngineConfig {
    pub fn load(path: &Path) -> Result<Self, AppError> {
        let text = fs::read_to_string(path).map_err(|source| AppError::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text, path)
    }

    /// Parses and validates; `origin` only labels errors.
    pub fn parse(text: &str, origin: &Path) -> Result<Self, AppError> {
        let config: Self = toml::from_str(text).map_err(|source| AppError::ConfigParse {
            path: origin.to_path_buf(),
            source: Box::new(source),
        })?;
        config.validate()?;
        Ok(config)
    }
}
