//! pricestabilizer: a chain-agnostic hybrid growth market-making engine.
//!
//! The engine only consumes generic market snapshots. Everything specific to
//! a chain or DEX lives outside it, in whatever produces those snapshots.

mod app;
mod config;
mod engine;
mod market;
mod report;

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(config_path) = std::env::args().nth(1) else {
        eprintln!("usage: pricestabilizer <config.toml>");
        return ExitCode::from(2);
    };
    match app::run::run(Path::new(&config_path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
