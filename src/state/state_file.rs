//! Loads and saves the engine state. Saving is atomic (write a temporary
//! file, flush it to disk, rename it over the old one), so the process can
//! be killed at any moment without leaving a torn file behind.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::app::error::AppError;
use crate::app::files::feed_file_name;
use crate::state::engine_state::{EngineState, STATE_VERSION};

/// One state file per feed inside the state directory.
pub fn path_for(directory: &Path, feed_name: &str) -> PathBuf {
    directory.join(format!("{}.state.json", feed_file_name(feed_name)))
}

/// `None` when no state was saved yet. A file that exists but cannot be read
/// stops the engine: starting fresh would forget a latched protection or a
/// spent budget.
pub fn load(path: &Path) -> Result<Option<EngineState>, AppError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(AppError::StateIo {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let state: EngineState =
        serde_json::from_str(&text).map_err(|source| AppError::StateParse {
            path: path.to_path_buf(),
            source,
        })?;
    if state.version != STATE_VERSION {
        return Err(AppError::StateVersion {
            path: path.to_path_buf(),
            found: state.version,
        });
    }
    Ok(Some(state))
}

pub fn save(path: &Path, state: &EngineState) -> Result<(), AppError> {
    write_atomically(path, state).map_err(|source| AppError::StateIo {
        path: path.to_path_buf(),
        source,
    })
}

fn write_atomically(path: &Path, state: &EngineState) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    let mut file = File::create(&temporary)?;
    serde_json::to_writer_pretty(&mut file, state).map_err(io::Error::other)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&temporary, path)
}
