//! Root of the configuration file.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::app::error::AppError;
use crate::config::engine_sections::{GrowthConfig, LiquidityConfig, PriceConfig, RegimeConfig};
use crate::config::feed_section::FeedConfig;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineConfig {
    pub growth: GrowthConfig,
    pub price: PriceConfig,
    pub regime: RegimeConfig,
    pub liquidity: LiquidityConfig,
    pub feed: FeedConfig,
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
