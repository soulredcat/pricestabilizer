//! What an operator can say, and what the engine did about it.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum OperatorCommand {
    /// Enter protection now.
    Pause,
    /// Leave protection, acknowledging losses so far.
    Resume,
    /// Adopt the venue's wallet balances in the ledger after transfers made
    /// outside the engine; protection still needs a `resume` afterwards.
    Reconcile,
    Approve {
        proposal: u64,
    },
    Reject {
        proposal: u64,
    },
}

/// One line of the command file: the command and when it takes effect.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ScheduledCommand {
    pub at: u64,
    #[serde(flatten)]
    pub command: OperatorCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorEvent {
    Paused,
    Resumed,
    /// Resume refused: the market is still in an emergency.
    ResumeRefused,
    Reconciled,
    /// The venue could not be read, so nothing was adjusted.
    ReconcileFailed,
}

impl OperatorEvent {
    pub fn label(self) -> &'static str {
        match self {
            Self::Paused => "operator_pause",
            Self::Resumed => "operator_resume",
            Self::ResumeRefused => "operator_resume_refused",
            Self::Reconciled => "operator_reconcile",
            Self::ReconcileFailed => "operator_reconcile_failed",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{OperatorCommand, ScheduledCommand};

    #[test]
    fn parses_each_command_shape() {
        let pause: ScheduledCommand =
            serde_json::from_str(r#"{"at": 5, "command": "pause"}"#).expect("pause");
        assert_eq!((pause.at, pause.command), (5, OperatorCommand::Pause));
        let approve: ScheduledCommand =
            serde_json::from_str(r#"{"at": 9, "command": "approve", "proposal": 2}"#)
                .expect("approve");
        assert_eq!(approve.command, OperatorCommand::Approve { proposal: 2 });
    }
}
