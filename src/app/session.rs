//! One trading session over one feed: operator commands, decision,
//! treasury, execution, and feedback into the next snapshot.

use crate::app::error::AppError;
use crate::config::engine_config::EngineConfig;
use crate::engine::strategy::decision::TickDecision;
use crate::engine::strategy::feedback::ExecutionFeedback;
use crate::engine::strategy::pipeline::StrategyPipeline;
use crate::execution::executor::Executor;
use crate::execution::outcome::ExecutionOutcome;
use crate::market::snapshot::MarketSnapshot;
use crate::operator::command::{OperatorCommand, OperatorEvent};
use crate::operator::inbox::CommandInbox;
use crate::portfolio::ledger::Ledger;
use crate::treasury::desk::TreasuryDesk;
use crate::treasury::proposal::TreasuryEvent;
use crate::venue::venue_factory::open_venue;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TickEvent {
    Operator(OperatorEvent),
    Treasury(TreasuryEvent),
}

impl TickEvent {
    pub fn label(self) -> &'static str {
        match self {
            Self::Operator(event) => event.label(),
            Self::Treasury(event) => event.label(),
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
}

impl TradingSession {
    pub fn open(config: &EngineConfig, feed_name: &str) -> Result<Self, AppError> {
        let executor = Executor::open(config, open_venue(config)).map_err(|error| {
            AppError::VenueUnavailable {
                feed: feed_name.to_owned(),
                reason: error.label(),
            }
        })?;
        Ok(Self {
            pipeline: StrategyPipeline::new(config),
            executor,
            treasury: TreasuryDesk::new(&config.treasury),
            inbox: CommandInbox::open(config.operator.commands_path.as_deref())?,
            feedback: ExecutionFeedback::default(),
        })
    }

    pub fn ledger(&self) -> &Ledger {
        self.executor.ledger()
    }

    pub fn step(&mut self, snapshot: &MarketSnapshot) -> Result<TickReport, AppError> {
        let mut events = Vec::new();
        for command in self.inbox.due(snapshot.timestamp)? {
            self.apply(command, &mut events);
        }
        let decision = self.pipeline.on_snapshot(snapshot, self.feedback);
        let mut treasury_events = Vec::new();
        let buy = self.treasury.on_tick(
            snapshot.timestamp,
            decision.mode,
            decision.price.is_trusted(),
            &mut treasury_events,
        );
        let outcome = self.executor.on_decision(
            snapshot,
            decision.liquidity,
            buy.map(|proposal| proposal.quote),
        );
        if let Some(proposal) = buy {
            self.treasury
                .settle(proposal, outcome.treasury_filled, &mut treasury_events);
        }
        events.extend(treasury_events.into_iter().map(TickEvent::Treasury));
        self.feedback = outcome.feedback;
        Ok(TickReport {
            decision,
            outcome,
            events,
        })
    }

    fn apply(&mut self, command: OperatorCommand, events: &mut Vec<TickEvent>) {
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
