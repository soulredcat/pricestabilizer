//! Builds the configured venue.

use std::time::Duration;

use crate::config::engine_config::EngineConfig;
use crate::config::venue_section::VenueConfig;
use crate::portfolio::asset::UnitScale;
use crate::venue::adapter::Venue;
use crate::venue::external::process_venue::ProcessVenue;
use crate::venue::order::VenueError;
use crate::venue::simulated::paper_venue::PaperVenue;

pub fn open_venue(config: &EngineConfig) -> Result<Box<dyn Venue>, VenueError> {
    match &config.venue {
        VenueConfig::Simulated(settings) => {
            let scale = UnitScale::new(config.assets.token_decimals, config.assets.quote_decimals);
            Ok(Box::new(PaperVenue::new(
                settings,
                config.price.depth_reference_move,
                scale,
            )))
        }
        VenueConfig::External(settings) => Ok(Box::new(ProcessVenue::spawn(
            &settings.command,
            Duration::from_secs(settings.timeout_secs),
        )?)),
    }
}
