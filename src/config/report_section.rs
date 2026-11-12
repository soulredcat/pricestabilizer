//! Optional outputs besides the printed summary.

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ReportConfig {
    /// When set, every feed's ledger is written here as JSON lines (audit).
    pub ledger_dir: Option<PathBuf>,
}
