//! Reads JSON-lines snapshots from a file or, live, from standard input: the
//! hand-off point for any external chain connector. Blank lines are skipped;
//! every other line is one snapshot.

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::app::error::AppError;
use crate::market::feed::stream::MarketFeed;
use crate::market::snapshot::MarketSnapshot;

pub fn open(path: &Path) -> Result<MarketFeed, AppError> {
    let file = File::open(path).map_err(|source| AppError::FeedRead {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(from_reader(
        path.display().to_string(),
        path.to_path_buf(),
        BufReader::new(file),
        None,
    ))
}

/// Snapshots piped in live by a connector, processed as each line arrives
/// and refused when further than `max_age_secs` from the wall clock.
pub fn stdin(max_age_secs: u64) -> MarketFeed {
    from_reader(
        "stdin".to_owned(),
        PathBuf::from("<stdin>"),
        io::stdin().lock(),
        Some(max_age_secs),
    )
}

fn from_reader(
    name: String,
    origin: PathBuf,
    reader: impl BufRead + 'static,
    clock_window_secs: Option<u64>,
) -> MarketFeed {
    let snapshots = reader
        .lines()
        .enumerate()
        .filter(|(_, line)| !matches!(line, Ok(text) if text.trim().is_empty()))
        .map(move |(index, line)| parse_line(&origin, index + 1, line));
    MarketFeed {
        name,
        snapshots: Box::new(snapshots),
        clock_window_secs,
    }
}

pub fn parse_line(
    path: &Path,
    line_number: usize,
    line: io::Result<String>,
) -> Result<MarketSnapshot, AppError> {
    let text = line.map_err(|source| AppError::FeedRead {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| AppError::FeedParse {
        path: path.to_path_buf(),
        line: line_number,
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::app::error::AppError;

    fn write_feed(name: &str, body: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("pricestabilizer-{name}.jsonl"));
        fs::write(&path, body).expect("temp file is writable");
        path
    }

    #[test]
    fn reads_snapshots_and_skips_blank_lines() {
        let path = write_feed(
            "ok",
            "{\"timestamp\":1,\"spot_price\":2.0,\"quote_depth\":5.0,\"quote_volume\":1.0,\"quote_usd_price\":1.0,\"block\":7}\n\n\
             {\"timestamp\":2,\"spot_price\":2.1,\"quote_depth\":5.0,\"quote_volume\":1.0,\"quote_usd_price\":1.0}\n",
        );
        let feed = super::open(&path).expect("feed opens");
        let prices: Vec<f64> = feed
            .snapshots
            .map(|snapshot| snapshot.expect("valid line").spot_price)
            .collect();
        assert_eq!(prices.len(), 2);
        assert!((prices[1] - 2.1).abs() < 1e-12);
    }

    #[test]
    fn reports_the_broken_line_number() {
        let path = write_feed("broken", "\n{\"timestamp\":1}\n");
        let mut feed = super::open(&path).expect("feed opens");
        let error = feed.snapshots.next().expect("one line").unwrap_err();
        assert!(matches!(error, AppError::FeedParse { line: 2, .. }));
    }
}
