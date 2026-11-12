//! Builds the configured venue.

use crate::config::engine_config::EngineConfig;
use crate::config::venue_section::VenueConfig;
use crate::portfolio::asset::UnitScale;
use crate::venue::adapter::Venue;
use crate::venue::simulated::paper_venue::PaperVenue;

pub fn open_venue(config: &EngineConfig) -> Box<dyn Venue> {
    let scale = UnitScale::new(config.assets.token_decimals, config.assets.quote_decimals);
    match &config.venue {
        VenueConfig::Simulated(settings) => Box::new(PaperVenue::new(
            settings,
            config.price.depth_reference_move,
            scale,
        )),
    }
}
