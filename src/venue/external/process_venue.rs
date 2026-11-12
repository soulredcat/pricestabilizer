//! A venue behind a connector process. Each call writes one request and
//! waits for its response; any protocol breach is a connector failure, and
//! repeated failures trip protection through the executor.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde::de::DeserializeOwned;

use crate::market::snapshot::MarketSnapshot;
use crate::portfolio::holding::Holding;
use crate::venue::adapter::Venue;
use crate::venue::external::protocol::{Call, Request, Response};
use crate::venue::order::{Fill, PositionView, VenueError, VenueOrder, Wallet};

pub struct ProcessVenue {
    child: Child,
    requests: ChildStdin,
    responses: BufReader<ChildStdout>,
    next_id: u64,
}

impl ProcessVenue {
    /// Starts `command[0]` with the remaining items as arguments.
    pub fn spawn(command: &[String]) -> Result<Self, VenueError> {
        let (program, arguments) = command.split_first().ok_or(VenueError::Connector)?;
        let mut child = Command::new(program)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| connector_failure("spawn", &error))?;
        let requests = child.stdin.take().ok_or(VenueError::Connector)?;
        let responses = child.stdout.take().ok_or(VenueError::Connector)?;
        Ok(Self {
            child,
            requests,
            responses: BufReader::new(responses),
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
        let mut line = String::new();
        let read = self
            .responses
            .read_line(&mut line)
            .map_err(|error| connector_failure("read", &error))?;
        if read == 0 {
            return Err(connector_failure("read", &"connector closed its output"));
        }
        let response: Response =
            serde_json::from_str(&line).map_err(|error| connector_failure("decode", &error))?;
        if response.id != id {
            return Err(connector_failure("decode", &"response id out of order"));
        }
        if let Some(code) = response.error {
            return Err(VenueError::from_code(&code));
        }
        serde_json::from_value(response.ok).map_err(|error| connector_failure("decode", &error))
    }
}

impl Venue for ProcessVenue {
    fn observe(&mut self, market: &MarketSnapshot) -> Result<(), VenueError> {
        self.call(Call::Observe { market: *market })
    }

    fn execute(&mut self, order: &VenueOrder, market: &MarketSnapshot) -> Result<Fill, VenueError> {
        self.call(Call::Execute {
            order: *order,
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
