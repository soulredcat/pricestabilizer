//! Optional outputs besides the printed summary.

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ReportConfig {
    /// When set, every feed's ledger entries are appended here as JSON lines
    /// as soon as they are booked (the audit journal).
    pub ledger_dir: Option<PathBuf>,
}
