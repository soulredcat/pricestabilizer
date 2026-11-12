//! File helpers: names derived from feed names (which may themselves be
//! paths), and live files that may not exist yet.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::Path;

/// Opens a file another process appends to, creating it (and its folder)
/// when it does not exist yet, so a fresh deployment starts cleanly.
pub fn open_or_create(path: &Path) -> io::Result<File> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(path)
}

pub fn feed_file_name(feed_name: &str) -> String {
    feed_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{feed_file_name, open_or_create};

    #[test]
    fn paths_become_flat_names() {
        assert_eq!(feed_file_name("data/feed.jsonl"), "data_feed_jsonl");
        assert_eq!(feed_file_name("stdin"), "stdin");
    }

    #[test]
    fn live_files_are_created_when_missing() {
        let path = std::env::temp_dir().join("pricestabilizer-files/new/commands.jsonl");
        std::fs::remove_dir_all(std::env::temp_dir().join("pricestabilizer-files")).ok();
        assert!(open_or_create(&path).is_ok());
        assert!(path.exists());
    }
}
