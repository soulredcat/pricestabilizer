//! One line per notable event, on stderr, as it happens. This is how an
//! operator learns a proposal's number in order to approve it.

use crate::app::session::TickEvent;
use crate::treasury::proposal::TreasuryEvent;

pub fn line(feed_name: &str, timestamp: u64, event: TickEvent) -> String {
    let detail = match event {
        TickEvent::Operator(_) => String::new(),
        TickEvent::Treasury(TreasuryEvent::Executed { id, quote }) => {
            format!(" #{id} spent {quote:.2} quote")
        }
        TickEvent::Treasury(treasury) => format!(" #{}", treasury.proposal_id()),
    };
    format!("[{feed_name} @ {timestamp}] {}{detail}", event.label())
}

#[cfg(test)]
mod tests {
    use super::line;
    use crate::app::session::TickEvent;
    use crate::operator::command::OperatorEvent;
    use crate::treasury::proposal::TreasuryEvent;

    #[test]
    fn names_the_proposal_an_operator_must_approve() {
        let proposed = TickEvent::Treasury(TreasuryEvent::Proposed(4));
        assert_eq!(line("feed", 9, proposed), "[feed @ 9] treasury_proposed #4");
        let paused = TickEvent::Operator(OperatorEvent::Paused);
        assert_eq!(line("feed", 9, paused), "[feed @ 9] operator_pause");
    }
}
