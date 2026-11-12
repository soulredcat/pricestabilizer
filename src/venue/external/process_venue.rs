//! A venue behind a connector process. Each call writes one request and
//! waits, at most `timeout`, for its response. A reader thread owns the
//! connector's stdout, so a hung connector can never block the engine.
//! Any protocol breach is a connector failure, and repeated failures trip
//! protection through the executor.

use std::io::{self, BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;

use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::holding::Holding;
use crate::venue::adapter::Venue;
use crate::venue::external::protocol::{Call, Request, Response};
use crate::venue::order::{Fill, OrderGuard, PositionView, VenueError, VenueOrder, Wallet};

pub struct ProcessVenue {
    child: Child,
    requests: ChildStdin,
    responses: Receiver<io::Result<String>>,
    timeout: Duration,
    next_id: u64,
}

impl ProcessVenue {
    /// Starts `command[0]` with the remaining items as arguments.
    pub fn spawn(command: &[String], timeout: Duration) -> Result<Self, VenueError> {
        let (program, arguments) = command.split_first().ok_or(VenueError::Connector)?;
        let mut child = Command::new(program)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| connector_failure("spawn", &error))?;
        let requests = child.stdin.take().ok_or(VenueError::Connector)?;
        let output = child.stdout.take().ok_or(VenueError::Connector)?;
        let (sender, responses) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            requests,
            responses,
            timeout,
            next_id: 1,
        })
    }

    fn call<T: DeserializeOwned>(&mut self, call: Call) -> Result<T, VenueError> {
        let id = self.next_id;
        self.next_id += 1;
        let request = serde_json::to_string(&Request { id, call })
            .map_err(|error| connector_failure("encode", &error))?;
        writeln!(self.requests, "{request}")
            .and_then(|()| self.requests.flush())
            .map_err(|error| connector_failure("write", &error))?;
        let response = self.await_response(id)?;
        if let Some(code) = response.error {
            return Err(VenueError::from_code(&code));
        }
        serde_json::from_value(response.ok).map_err(|error| connector_failure("decode", &error))
    }

    /// Waits for the response to `id`. A late response to an earlier,
    /// timed-out request is logged and dropped: that request may still have
    /// executed, which the next reconciliation will catch.
    fn await_response(&mut self, id: u64) -> Result<Response, VenueError> {
        let deadline = Instant::now() + self.timeout;
        loop {
            let waited = deadline.saturating_duration_since(Instant::now());
            let line = match self.responses.recv_timeout(waited) {
                Ok(Ok(line)) => line,
                Ok(Err(error)) => return Err(connector_failure("read", &error)),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(connector_failure(
                        "read",
                        &"timed out; the call may still complete",
                    ));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(connector_failure("read", &"connector closed its output"));
                }
            };
            let response: Response =
                serde_json::from_str(&line).map_err(|error| connector_failure("decode", &error))?;
            if response.id < id {
                eprintln!(
                    "connector: dropped late response to request {}",
                    response.id
                );
                continue;
            }
            if response.id != id {
                return Err(connector_failure("decode", &"response id out of order"));
            }
            return Ok(response);
        }
    }
}

impl Venue for ProcessVenue {
    fn observe(&mut self, market: &MarketSnapshot) -> Result<(), VenueError> {
        self.call(Call::Observe { market: *market })
    }

    fn execute(
        &mut self,
        order: &VenueOrder,
        guard: &OrderGuard,
        market: &MarketSnapshot,
    ) -> Result<Fill, VenueError> {
        self.call(Call::Execute {
            order: *order,
            guard: *guard,
            market: *market,
        })
    }

    fn balances(&mut self, wallet: Wallet) -> Result<Holding, VenueError> {
        self.call(Call::Balances { wallet })
    }

    fn position(&mut self, market: &MarketSnapshot) -> Result<Option<PositionView>, VenueError> {
        self.call(Call::Position { market: *market })
    }
}

impl Drop for ProcessVenue {
    /// Stops the connector with the engine. Errors are moot at this point:
    /// the process may already have exited on its own.
    fn drop(&mut self) {
        if self.child.kill().is_ok() {
            self.child.wait().ok();
        }
    }
}

/// Logs what went wrong, since the venue error itself carries no detail.
fn connector_failure(stage: &str, detail: &dyn std::fmt::Display) -> VenueError {
    eprintln!("connector {stage} failed: {detail}");
    VenueError::Connector
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::ProcessVenue;
    use crate::venue::adapter::Venue;
    use crate::venue::order::{VenueError, Wallet};

    #[test]
    fn a_silent_connector_times_out_instead_of_hanging() {
        let command = ["sleep".to_owned(), "5".to_owned()];
        let mut venue =
            ProcessVenue::spawn(&command, Duration::from_millis(200)).expect("connector starts");
        let started = Instant::now();
        assert_eq!(
            venue.balances(Wallet::Operational),
            Err(VenueError::Connector)
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
