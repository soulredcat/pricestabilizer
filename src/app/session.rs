//! One trading session over one feed: operator commands, decision,
//! treasury, execution, and feedback into the next snapshot. With a state
//! directory, the session resumes where it stopped and saves after every
//! snapshot.

use std::path::PathBuf;

use crate::app::error::AppError;
use crate::config::engine_config::EngineConfig;
use crate::engine::mode::operating_mode::OperatingMode;
use crate::engine::regime::market_regime::EmergencyCause;
use crate::engine::strategy::decision::TickDecision;
use crate::engine::strategy::feedback::ExecutionFeedback;
use crate::engine::strategy::pipeline::StrategyPipeline;
use crate::execution::executor::Executor;
use crate::execution::outcome::ExecutionOutcome;
use crate::execution::request::ExecutionRequest;
use crate::market::snapshot::MarketSnapshot;
use crate::operator::command::{OperatorCommand, OperatorEvent};
use crate::operator::inbox::CommandInbox;
use crate::portfolio::ledger::Ledger;
use crate::state::engine_state::{EngineState, STATE_VERSION};
use crate::state::state_file;
use crate::treasury::desk::TreasuryDesk;
use crate::treasury::proposal::TreasuryEvent;
use crate::venue::venue_factory::open_venue;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TickEvent {
    Operator(OperatorEvent),
    Treasury(TreasuryEvent),
    /// Protection just latched.
    Protection(EmergencyCause),
}

impl TickEvent {
    pub fn label(self) -> &'static str {
        match self {
            Self::Operator(event) => event.label(),
            Self::Treasury(event) => event.label(),
            Self::Protection(_) => "protection_latched",
        }
    }
}

pub struct TickReport {
    pub decision: TickDecision,
    pub outcome: ExecutionOutcome,
    pub events: Vec<TickEvent>,
}

pub struct TradingSession {
    pipeline: StrategyPipeline,
    executor: Executor,
    treasury: TreasuryDesk,
    inbox: CommandInbox,
    feedback: ExecutionFeedback,
    protected: bool,
    state_path: Option<PathBuf>,
    resumed_at: Option<u64>,
}

impl TradingSession {
    pub fn open(config: &EngineConfig, feed_name: &str) -> Result<Self, AppError> {
        let state_path = config
            .state
            .dir
            .as_deref()
            .map(|directory| state_file::path_for(directory, feed_name));
        let saved = match &state_path {
            Some(path) => state_file::load(path)?,
            None => None,
        };
        let executor = open_venue(config)
            .and_then(|venue| {
                Executor::open(
                    config,
                    venue,
                    saved.as_ref().map(|state| state.execution.clone()),
                )
            })
            .map_err(|error| AppError::VenueUnavailable {
                feed: feed_name.to_owned(),
                reason: error.label(),
            })?;
        let mut pipeline = StrategyPipeline::new(config);
        let mut treasury = TreasuryDesk::new(&config.treasury);
        if let Some(state) = &saved {
            pipeline.restore(state.pipeline);
            treasury.restore(state.treasury);
        }
        let consumed = saved.as_ref().map_or(0, |state| state.commands_consumed);
        Ok(Self {
            pipeline,
            executor,
            treasury,
            inbox: CommandInbox::open(config.operator.commands_path.as_deref(), consumed)?,
            feedback: ExecutionFeedback::default(),
            protected: false,
            state_path,
            resumed_at: saved.map(|state| state.saved_at),
        })
    }

    /// The last snapshot handled before a restart; older ones are refused.
    pub fn resumed_at(&self) -> Option<u64> {
        self.resumed_at
    }

    pub fn ledger(&self) -> &Ledger {
        self.executor.ledger()
    }

    pub fn step(&mut self, snapshot: &MarketSnapshot) -> Result<TickReport, AppError> {
        let mut events = Vec::new();
        for command in self.inbox.due(snapshot.timestamp)? {
            self.apply(command, snapshot.timestamp, &mut events);
        }
        let decision = self.pipeline.on_snapshot(snapshot, self.feedback);
        let protection = match decision.mode {
            OperatingMode::Protection(cause) => Some(cause),
            _ => None,
        };
        if let (Some(cause), false) = (protection, self.protected) {
            events.push(TickEvent::Protection(cause));
        }
        self.protected = protection.is_some();
        let mut treasury_events = Vec::new();
        let buy = self.treasury.on_tick(
            snapshot.timestamp,
            decision.mode,
            decision.price.is_trusted(),
            &mut treasury_events,
        );
        let request = ExecutionRequest {
            action: decision.liquidity,
            capital_share: decision.capital_share,
            treasury_buy_quote: buy.map(|proposal| proposal.quote),
            valuation_price: decision.price.twap,
        };
        let outcome = self.executor.on_decision(snapshot, &request);
        if let Some(proposal) = buy {
            self.treasury
                .settle(proposal, outcome.treasury_spent, &mut treasury_events);
        }
        events.extend(treasury_events.into_iter().map(TickEvent::Treasury));
        self.feedback = outcome.feedback;
        self.save(snapshot.timestamp)?;
        Ok(TickReport {
            decision,
            outcome,
            events,
        })
    }

    fn save(&self, timestamp: u64) -> Result<(), AppError> {
        let Some(path) = &self.state_path else {
            return Ok(());
        };
        let state = EngineState {
            version: STATE_VERSION,
            saved_at: timestamp,
            pipeline: self.pipeline.state(),
            execution: self.executor.state(),
            treasury: self.treasury.state(),
            commands_consumed: self.inbox.consumed(),
        };
        state_file::save(path, &state)
    }

    fn apply(&mut self, command: OperatorCommand, timestamp: u64, events: &mut Vec<TickEvent>) {
        let event = match command {
            OperatorCommand::Pause => {
                self.pipeline.pause();
                TickEvent::Operator(OperatorEvent::Paused)
            }
            OperatorCommand::Resume if self.pipeline.resume() => {
                self.executor.acknowledge_losses();
                self.feedback.alert = None;
                TickEvent::Operator(OperatorEvent::Resumed)
            }
            OperatorCommand::Resume => TickEvent::Operator(OperatorEvent::ResumeRefused),
            OperatorCommand::Reconcile if self.executor.reconcile(timestamp) => {
                TickEvent::Operator(OperatorEvent::Reconciled)
            }
            OperatorCommand::Reconcile => TickEvent::Operator(OperatorEvent::ReconcileFailed),
            OperatorCommand::Approve { proposal } => match self.treasury.approve(proposal) {
                Some(event) => TickEvent::Treasury(event),
                None => return,
            },
            OperatorCommand::Reject { proposal } => {
                TickEvent::Treasury(self.treasury.reject(proposal))
            }
        };
        events.push(event);
    }
}
