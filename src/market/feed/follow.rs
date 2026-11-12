//! Follows a JSON-lines file as it grows, like `tail -f`: a connector in
//! another process or container appends snapshots and the engine handles
//! each complete line as it lands. The feed never ends on its own. If the
//! file is truncated or replaced by a shorter one, reading starts over from
//! its beginning; snapshots already handled are then refused as out of order.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use crate::app::error::AppError;
use crate::app::files::open_or_create;
use crate::market::feed::jsonl::parse_line;
use crate::market::feed::stream::MarketFeed;

pub fn follow(path: &Path, poll: Duration, max_age_secs: u64) -> Result<MarketFeed, AppError> {
    let lines = FollowedLines {
        reader: open(path)?,
        path: path.to_path_buf(),
        poll,
        partial: String::new(),
        position: 0,
        line_number: 0,
    };
    let origin = path.to_path_buf();
    Ok(MarketFeed {
        name: path.display().to_string(),
        snapshots: Box::new(
            lines.map(move |(line_number, line)| parse_line(&origin, line_number, line)),
        ),
        clock_window_secs: Some(max_age_secs),
    })
}

/// The connector may not have written anything yet: start on an empty file.
fn open(path: &Path) -> Result<BufReader<File>, AppError> {
    open_or_create(path)
        .map(BufReader::new)
        .map_err(|source| AppError::FeedRead {
            path: path.to_path_buf(),
            source,
        })
}

struct FollowedLines {
    reader: BufReader<File>,
    path: PathBuf,
    poll: Duration,
    partial: String,
    /// Bytes consumed so far, to notice a truncated file.
    position: u64,
    line_number: usize,
}

impl FollowedLines {
    /// Starts over when the file became shorter than what was read.
    fn restart_if_truncated(&mut self) {
        let shorter = fs::metadata(&self.path).is_ok_and(|meta| meta.len() < self.position);
        if let (true, Ok(file)) = (shorter, File::open(&self.path)) {
            eprintln!(
                "[{}] file was truncated; reading from the start",
                self.path.display()
            );
            self.reader = BufReader::new(file);
            self.partial.clear();
            self.position = 0;
        }
    }
}

impl Iterator for FollowedLines {
    type Item = (usize, io::Result<String>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.reader.read_line(&mut self.partial) {
                Ok(read) if read > 0 && self.partial.ends_with('\n') => {
                    self.position += read as u64;
                    self.line_number += 1;
                    let line = std::mem::take(&mut self.partial);
                    if !line.trim().is_empty() {
                        return Some((self.line_number, Ok(line)));
                    }
                }
                Ok(read) => {
                    // Nothing new, or half a line still being written.
                    self.position += read as u64;
                    self.restart_if_truncated();
                    thread::sleep(self.poll);
                }
                Err(error) => return Some((self.line_number + 1, Err(error))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::thread;
    use std::time::Duration;

    use super::follow;

    const LINE: &str = "{\"timestamp\":7,\"spot_price\":1.0,\"quote_depth\":5.0,\"quote_volume\":1.0,\"quote_usd_price\":1.0}\n";

    #[test]
    fn waits_for_lines_appended_later() {
        let path = std::env::temp_dir().join("pricestabilizer-follow.jsonl");
        fs::write(&path, LINE).expect("writable");
        let writer_path = path.clone();
        let writer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            let mut file = OpenOptions::new()
                .append(true)
                .open(writer_path)
                .expect("appendable");
            // Half a line first: it must not be parsed until it is complete.
            file.write_all(&LINE.as_bytes()[..20]).expect("written");
            file.flush().expect("flushed");
            thread::sleep(Duration::from_millis(50));
            file.write_all(&LINE.as_bytes()[20..]).expect("written");
        });
        let feed = follow(&path, Duration::from_millis(5), 60).expect("follows");
        let timestamps: Vec<u64> = feed
            .snapshots
            .take(2)
            .map(|snapshot| snapshot.expect("valid").timestamp)
            .collect();
        writer.join().expect("writer finished");
        assert_eq!(timestamps, vec![7, 7]);
        assert_eq!(feed.clock_window_secs, Some(60));
    }
}
