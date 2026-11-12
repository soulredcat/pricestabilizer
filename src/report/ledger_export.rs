//! Writes a feed's ledger as JSON lines, one entry per line, for audit.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::app::error::AppError;
use crate::portfolio::ledger::Ledger;

pub fn write(directory: &Path, feed_name: &str, ledger: &Ledger) -> Result<PathBuf, AppError> {
    let file_name: String = feed_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = directory.join(format!("{file_name}.ledger.jsonl"));
    write_entries(&path, ledger).map_err(|source| AppError::LedgerExport {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

fn write_entries(path: &Path, ledger: &Ledger) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut out = BufWriter::new(File::create(path)?);
    for entry in ledger.entries() {
        serde_json::to_writer(&mut out, entry).map_err(io::Error::other)?;
        out.write_all(b"\n")?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use crate::portfolio::holding::Holding;
    use crate::portfolio::ledger::{Account, Ledger};

    #[test]
    fn writes_one_json_line_per_entry_with_string_units() {
        let mut ledger = Ledger::default();
        ledger.deposit(
            5,
            Account::Treasury,
            Holding {
                token: 0,
                quote: 42,
            },
        );
        let directory = std::env::temp_dir().join("pricestabilizer-ledger-test");
        let path = super::write(&directory, "data/feed.jsonl", &ledger).expect("written");
        assert!(path.ends_with("data_feed_jsonl.ledger.jsonl"));
        let text = std::fs::read_to_string(path).expect("readable");
        assert_eq!(
            text,
            "{\"timestamp\":5,\"kind\":\"deposit\",\"asset\":\"quote\",\"from\":\"capital\",\"to\":\"treasury\",\"units\":\"42\"}\n"
        );
    }
}
