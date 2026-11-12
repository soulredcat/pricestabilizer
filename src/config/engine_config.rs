//! Root of the configuration file.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::app::error::AppError;
use crate::config::engine_sections::{GrowthConfig, LiquidityConfig, PriceConfig, RegimeConfig};
use crate::config::execution_sections::{AssetsConfig, RiskConfig};
use crate::config::feed_section::FeedConfig;
use crate::config::report_section::ReportConfig;
use crate::config::state_section::StateConfig;
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
    #[serde(default)]
    pub state: StateConfig,
}

impl EngineConfig {
    pub fn load(path: &Path) -> Result<Self, AppError> {
        let text = fs::read_to_string(path).map_err(|source| AppError::ConfigRead {
            path: path.to_path_buf(),
            source,
        })?;
        Self::parse(&text, path)
    }

    /// One line on what this config will do, for `check-config`.
    pub fn describe(&self) -> String {
        let feed = match &self.feed {
            FeedConfig::Scenario(settings) => format!("{} scenarios", settings.scenarios.len()),
            FeedConfig::Jsonl(settings) => format!("file {}", settings.path.display()),
            FeedConfig::Stdin(_) => "live on stdin".to_owned(),
            FeedConfig::Follow(settings) => format!("live from {}", settings.path.display()),
        };
        let venue = match &self.venue {
            VenueConfig::Simulated(_) => "paper venue".to_owned(),
            VenueConfig::External(settings) if settings.send_orders => {
                format!("external venue {:?}, SENDING orders", settings.command)
            }
            VenueConfig::External(settings) => {
                format!("external venue {:?}, orders withheld", settings.command)
            }
        };
        let state = self
            .state
            .dir
            .as_ref()
            .map_or("no state".to_owned(), |dir| {
                format!("state in {}", dir.display())
            });
        format!("feed {feed}; {venue}; {state}")
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
