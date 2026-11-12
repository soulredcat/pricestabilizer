//! pricestabilizer: a chain-agnostic hybrid growth market-making engine.
//!
//! The engine only consumes generic market snapshots and talks to venues
//! through one trait. Everything specific to a chain lives outside it, in
//! connectors that produce snapshots and execute orders.

mod app;
mod config;
mod engine;
mod execution;
mod market;
mod operator;
mod portfolio;
mod report;
mod treasury;
mod venue;

use std::path::Path;
use std::process::ExitCode;

use crate::app::error::AppError;
use crate::config::engine_config::EngineConfig;

const USAGE: &str =
    "usage: pricestabilizer <config.toml>\n       pricestabilizer paper-connector <config.toml>";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.as_slice() {
        [config_path] => app::run::run(Path::new(config_path)),
        [mode, config_path] if mode == "paper-connector" => serve_paper(Path::new(config_path)),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the reference connector: the paper venue over stdin/stdout.
fn serve_paper(config_path: &Path) -> Result<(), AppError> {
    venue::external::paper_server::serve(&EngineConfig::load(config_path)?)
}
