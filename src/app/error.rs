//! The single error type surfaced to the command line.

use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum AppError {
    ConfigRead {
        path: PathBuf,
        source: io::Error,
    },
    ConfigParse {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },
    ConfigInvalid {
        field: &'static str,
        rule: &'static str,
    },
    FeedRead {
        path: PathBuf,
        source: io::Error,
    },
    FeedParse {
        path: PathBuf,
        line: usize,
        source: serde_json::Error,
    },
    VenueUnavailable {
        feed: String,
        reason: &'static str,
    },
    LedgerExport {
        path: PathBuf,
        source: io::Error,
    },
    ConnectorIo {
        source: io::Error,
    },
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigRead { path, source } => {
                write!(f, "cannot read config {}: {source}", path.display())
            }
            Self::ConfigParse { path, source } => {
                write!(f, "invalid config {}: {source}", path.display())
            }
            Self::ConfigInvalid { field, rule } => write!(f, "config field `{field}` {rule}"),
            Self::FeedRead { path, source } => {
                write!(f, "cannot read feed {}: {source}", path.display())
            }
            Self::FeedParse { path, line, source } => {
                write!(f, "feed {} line {line}: {source}", path.display())
            }
            Self::VenueUnavailable { feed, reason } => {
                write!(f, "feed {feed}: venue unavailable ({reason})")
            }
            Self::LedgerExport { path, source } => {
                write!(f, "cannot write ledger {}: {source}", path.display())
            }
            Self::ConnectorIo { source } => write!(f, "connector stdio failed: {source}"),
        }
    }
}
