//! Appends every ledger entry, as soon as it is booked, to a JSON-lines
//! journal: the audit trail. A resumed session appends to the journal it
//! left behind; a fresh one starts a new file.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::app::error::AppError;
use crate::app::files::feed_file_name;
use crate::portfolio::ledger::Ledger;

pub struct LedgerJournal {
    path: PathBuf,
    out: BufWriter<File>,
    written: usize,
}

impl LedgerJournal {
    pub fn open(directory: &Path, feed_name: &str, resuming: bool) -> Result<Self, AppError> {
        let path = directory.join(format!("{}.ledger.jsonl", feed_file_name(feed_name)));
        let file =
            open_file(directory, &path, resuming).map_err(|source| AppError::LedgerJournal {
                path: path.clone(),
                source,
            })?;
        Ok(Self {
            path,
            out: BufWriter::new(file),
            written: 0,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Writes the entries booked since the last call, then flushes.
    pub fn record(&mut self, ledger: &Ledger) -> Result<(), AppError> {
        let entries = &ledger.entries()[self.written..];
        let written = entries
            .iter()
            .try_for_each(|entry| {
                serde_json::to_writer(&mut self.out, entry).map_err(io::Error::other)?;
                self.out.write_all(b"\n")
            })
            .and_then(|()| self.out.flush());
        written.map_err(|source| AppError::LedgerJournal {
            path: self.path.clone(),
            source,
        })?;
        self.written = ledger.entries().len();
        Ok(())
    }
}

fn open_file(directory: &Path, path: &Path, resuming: bool) -> io::Result<File> {
    fs::create_dir_all(directory)?;
    let mut options = OpenOptions::new();
    if resuming {
        options.create(true).append(true);
    } else {
        options.create(true).write(true).truncate(true);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::LedgerJournal;
    use crate::portfolio::holding::Holding;
    use crate::portfolio::ledger::{Account, Ledger};

    #[test]
    fn writes_each_entry_once_and_appends_when_resuming() {
        let directory = std::env::temp_dir().join("pricestabilizer-journal-test");
        let mut ledger = Ledger::default();
        ledger.deposit(
            5,
            Account::Treasury,
            Holding {
                token: 0,
                quote: 42,
            },
        );
        let mut journal = LedgerJournal::open(&directory, "data/feed.jsonl", false).expect("opens");
        journal.record(&ledger).expect("writes");
        journal.record(&ledger).expect("writes nothing new");
        assert!(journal.path().ends_with("data_feed_jsonl.ledger.jsonl"));
        let line = "{\"timestamp\":5,\"kind\":\"deposit\",\"asset\":\"quote\",\"from\":\"capital\",\"to\":\"treasury\",\"units\":\"42\"}\n";
        assert_eq!(
            std::fs::read_to_string(journal.path()).expect("readable"),
            line
        );

        let mut resumed = LedgerJournal::open(&directory, "data/feed.jsonl", true).expect("opens");
        resumed.record(&ledger).expect("appends");
        let text = std::fs::read_to_string(resumed.path()).expect("readable");
        assert_eq!(text, format!("{line}{line}"));
    }
}
