//! Reads operator commands from a JSON-lines file. The file is followed like
//! a log: lines appended while the engine runs arrive on the next snapshot,
//! and a command takes effect once a snapshot reaches its `at` time. Lines
//! that do not parse are reported on stderr and skipped.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::app::error::AppError;
use crate::app::files::open_or_create;
use crate::operator::command::{OperatorCommand, ScheduledCommand};

#[derive(Default)]
pub struct CommandInbox {
    source: Option<(PathBuf, BufReader<File>)>,
    partial_line: String,
    lines_read: usize,
    /// Lines handled before a restart; they are never applied twice.
    already_consumed: usize,
    queued: Vec<ScheduledCommand>,
}

impl CommandInbox {
    /// An inbox without a path never delivers anything. The first
    /// `already_consumed` lines were handled before a restart and are skipped.
    pub fn open(path: Option<&Path>, already_consumed: usize) -> Result<Self, AppError> {
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let file = open_or_create(path).map_err(|source| AppError::FeedRead {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(Self {
            source: Some((path.to_path_buf(), BufReader::new(file))),
            already_consumed,
            ..Self::default()
        })
    }

    /// Lines read so far, counting those skipped after a restart. A queued
    /// command counts as consumed: it is saved with the state it will change.
    pub fn consumed(&self) -> usize {
        self.lines_read.max(self.already_consumed)
    }

    /// Commands whose time has come, oldest first.
    pub fn due(&mut self, now: u64) -> Result<Vec<OperatorCommand>, AppError> {
        self.read_new_lines()?;
        self.queued.sort_by_key(|scheduled| scheduled.at);
        let ready = self.queued.partition_point(|scheduled| scheduled.at <= now);
        Ok(self
            .queued
            .drain(..ready)
            .map(|scheduled| scheduled.command)
            .collect())
    }

    fn read_new_lines(&mut self) -> Result<(), AppError> {
        let Some((path, reader)) = self.source.as_mut() else {
            return Ok(());
        };
        loop {
            let read =
                reader
                    .read_line(&mut self.partial_line)
                    .map_err(|source| AppError::FeedRead {
                        path: path.clone(),
                        source,
                    })?;
            // A line without its newline is still being written: wait for it.
            if read == 0 || !self.partial_line.ends_with('\n') {
                return Ok(());
            }
            self.lines_read += 1;
            let line = std::mem::take(&mut self.partial_line);
            if self.lines_read <= self.already_consumed || line.trim().is_empty() {
                continue;
            }
            // A typo must not stop a live engine: report it and read on.
            match serde_json::from_str(&line) {
                Ok(scheduled) => self.queued.push(scheduled),
                Err(error) => eprintln!(
                    "[operator] ignored {} line {}: {error}",
                    path.display(),
                    self.lines_read
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Write;

    use super::CommandInbox;
    use crate::operator::command::OperatorCommand;

    #[test]
    fn delivers_commands_in_time_order_and_follows_appends() {
        let path = std::env::temp_dir().join("pricestabilizer-inbox.jsonl");
        fs::write(
            &path,
            "{\"at\":20,\"command\":\"resume\"}\n{\"at\":10,\"command\":\"pause\"}\n",
        )
        .expect("writable");
        let mut inbox = CommandInbox::open(Some(&path), 0).expect("opens");
        assert!(inbox.due(5).expect("reads").is_empty());
        assert_eq!(inbox.due(15).expect("reads"), vec![OperatorCommand::Pause]);

        let mut file = OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("appendable");
        file.write_all(b"{\"at\":12,\"command\":\"reject\",\"proposal\":1}\n{\"at\":1,")
            .expect("appended");
        assert_eq!(
            inbox.due(25).expect("reads"),
            vec![
                OperatorCommand::Reject { proposal: 1 },
                OperatorCommand::Resume
            ]
        );
    }

    #[test]
    fn lines_consumed_before_a_restart_are_never_applied_again() {
        let path = std::env::temp_dir().join("pricestabilizer-inbox-restart.jsonl");
        fs::write(
            &path,
            "{\"at\":1,\"command\":\"approve\",\"proposal\":1}\n{\"at\":2,\"command\":\"pause\"}\n",
        )
        .expect("writable");
        let mut inbox = CommandInbox::open(Some(&path), 1).expect("opens");
        assert_eq!(inbox.due(10).expect("reads"), vec![OperatorCommand::Pause]);
        assert_eq!(inbox.consumed(), 2);
    }

    #[test]
    fn no_path_means_no_commands() {
        assert!(
            CommandInbox::open(None, 0)
                .expect("opens")
                .due(u64::MAX)
                .expect("reads")
                .is_empty()
        );
    }
}
