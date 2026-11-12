//! Serves the paper venue over the connector protocol on stdin/stdout.
//! It is the reference connector: a connector for a real chain must answer
//! the same calls the same way.

use std::io::{self, BufRead, Write};

use serde_json::Value;

use crate::app::error::AppError;
use crate::config::engine_config::EngineConfig;
use crate::config::venue_section::VenueConfig;
use crate::portfolio::asset::UnitScale;
use crate::venue::adapter::Venue;
use crate::venue::external::protocol::{Call, Request, Response};
use crate::venue::order::VenueError;
use crate::venue::simulated::paper_venue::PaperVenue;

pub fn serve(config: &EngineConfig) -> Result<(), AppError> {
    let VenueConfig::Simulated(settings) = &config.venue else {
        return Err(AppError::ConfigInvalid {
            field: "venue.kind",
            rule: "must be \"simulated\" for the paper connector",
        });
    };
    let scale = UnitScale::new(config.assets.token_decimals, config.assets.quote_decimals);
    let mut venue = PaperVenue::new(settings, config.price.depth_reference_move, scale);
    let mut output = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|source| AppError::ConnectorIo { source })?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => answer(&mut venue, request),
            Err(error) => Response {
                id: 0,
                ok: Value::Null,
                error: Some(format!("bad_request: {error}")),
            },
        };
        let text = serde_json::to_string(&response).map_err(|error| AppError::ConnectorIo {
            source: io::Error::other(error),
        })?;
        writeln!(output, "{text}")
            .and_then(|()| output.flush())
            .map_err(|source| AppError::ConnectorIo { source })?;
    }
    Ok(())
}

fn answer(venue: &mut PaperVenue, request: Request) -> Response {
    let result = match request.call {
        Call::Observe { market } => venue.observe(&market).map(|()| Value::Null),
        Call::Execute {
            order,
            guard,
            market,
        } => venue.execute(&order, &guard, &market).and_then(to_value),
        Call::Balances { wallet } => venue.balances(wallet).and_then(to_value),
        Call::Position { market } => venue.position(&market).and_then(to_value),
    };
    match result {
        Ok(ok) => Response {
            id: request.id,
            ok,
            error: None,
        },
        Err(error) => Response {
            id: request.id,
            ok: Value::Null,
            error: Some(error.code().to_owned()),
        },
    }
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value, VenueError> {
    serde_json::to_value(value).map_err(|_| VenueError::Connector)
}
