//! Where the engine keeps what must survive a restart.

use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct StateConfig {
    /// Directory for one state file per feed, saved after every snapshot.
    /// Required when an external venue sends orders.
    pub dir: Option<PathBuf>,
}
