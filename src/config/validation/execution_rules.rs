//! Checks for assets, risk, venue and feed sections.

use crate::app::error::AppError;
use crate::config::execution_sections::{AssetsConfig, RiskConfig};
use crate::config::feed_section::FeedConfig;
use crate::config::validation::rules::{check, fraction, non_negative, nonzero, positive, rate};
use crate::config::venue_section::VenueConfig;

/// More decimals than this cannot round-trip through `f64` amounts.
const MAX_DECIMALS: u32 = 30;

pub fn check_assets(assets: &AssetsConfig) -> Result<(), AppError> {
    let rule = "must be at most 30";
    check(
        assets.token_decimals <= MAX_DECIMALS,
        "assets.token_decimals",
        rule,
    )?;
    check(
        assets.quote_decimals <= MAX_DECIMALS,
        "assets.quote_decimals",
        rule,
    )
}

pub fn check_risk(risk: &RiskConfig) -> Result<(), AppError> {
    positive("risk.max_swap_quote", risk.max_swap_quote)?;
    fraction("risk.max_slippage", risk.max_slippage)?;
    non_negative("risk.min_quote_reserve", risk.min_quote_reserve)?;
    positive(
        "risk.max_token_exposure_quote",
        risk.max_token_exposure_quote,
    )?;
    positive("risk.max_daily_loss_quote", risk.max_daily_loss_quote)?;
    fraction("risk.max_drawdown", risk.max_drawdown)?;
    nonzero(
        "risk.max_consecutive_failures",
        u64::from(risk.max_consecutive_failures),
    )
}

pub fn check_venue(venue: &VenueConfig) -> Result<(), AppError> {
    match venue {
        VenueConfig::Simulated(settings) => {
            rate("venue.fee_rate", settings.fee_rate)?;
            rate("venue.swap_fee_rate", settings.swap_fee_rate)?;
            non_negative("venue.action_cost_quote", settings.action_cost_quote)?;
            non_negative("venue.operational_token", settings.operational_token)?;
            non_negative("venue.operational_quote", settings.operational_quote)?;
            non_negative("venue.treasury_token", settings.treasury_token)?;
            non_negative("venue.treasury_quote", settings.treasury_quote)
        }
    }
}

pub fn check_feed(feed: &FeedConfig) -> Result<(), AppError> {
    match feed {
        FeedConfig::Scenario(settings) => {
            check(
                !settings.scenarios.is_empty(),
                "feed.scenarios",
                "must list at least one scenario",
            )?;
            nonzero("feed.steps", settings.steps)?;
            nonzero("feed.interval_secs", settings.interval_secs)?;
            positive("feed.start_price", settings.start_price)?;
            positive("feed.start_quote_depth", settings.start_quote_depth)
        }
        FeedConfig::Jsonl(_) => Ok(()),
    }
}
