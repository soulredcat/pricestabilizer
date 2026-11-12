//! Plain-text rendering of a run summary.

use crate::report::run_summary::{Counter, RunSummary};

const SECONDS_PER_DAY: f64 = 86_400.0;

pub fn render(summary: &RunSummary) -> String {
    let days = summary.elapsed_secs as f64 / SECONDS_PER_DAY;
    let mut lines = vec![format!(
        "== {} == {} ticks over {days:.2} days",
        summary.feed_name, summary.ticks
    )];
    if summary.ticks == 0 {
        lines.push("  feed was empty".to_owned());
        return lines.join("\n");
    }
    lines.push(format!(
        "  price        start {:.4}  end {:.4}  target {:.4}  vs target {:+.2}% (low {:+.2}%, high {:+.2}%)",
        summary.first_spot,
        summary.last_spot,
        summary.last_target,
        percent(summary.last_spot / summary.last_target - 1.0),
        percent(summary.lowest_target_gap),
        percent(summary.highest_target_gap),
    ));
    lines.push(format!(
        "  regimes      {}",
        shares(&summary.regimes, summary.ticks)
    ));
    lines.push(format!(
        "  modes        {}",
        shares(&summary.modes, summary.ticks)
    ));
    lines.push(format!("  price flags  {}", counts(&summary.price_issues)));
    lines.push(format!("  skipped      {}", counts(&summary.skipped)));
    lines.push(format!("  liquidity    {}", counts(&summary.liquidity)));
    lines.push(format!("  band         {}", band_line(summary)));
    lines.push(format!("  orders       {}", counts(&summary.orders)));
    lines.push(format!("  books        {}", books_line(summary)));
    lines.push(format!(
        "  events       {}; treasury spent {:.2}",
        counts(&summary.events),
        summary.treasury_spent
    ));
    lines.push(format!("  protection   {}", protection_line(summary)));
    lines.join("\n")
}

fn percent(ratio: f64) -> f64 {
    ratio * 100.0
}

fn shares(counter: &Counter, total: u64) -> String {
    joined(counter, |count| {
        format!("{:.2}%", count as f64 / total as f64 * 100.0)
    })
}

fn counts(counter: &Counter) -> String {
    joined(counter, |count| count.to_string())
}

fn joined(counter: &Counter, value: impl Fn(u64) -> String) -> String {
    if counter.is_empty() {
        return "none".to_owned();
    }
    counter
        .iter()
        .map(|(label, count)| format!("{label} {}", value(*count)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn band_line(summary: &RunSummary) -> String {
    match summary.latest_band {
        None => "never opened".to_owned(),
        Some(band) => format!(
            "last [{:.4} .. {:.4}], spot inside it {:.1}% of the time",
            band.lower,
            band.upper,
            summary.ticks_spot_in_band as f64 / summary.ticks_with_band as f64 * 100.0
        ),
    }
}

fn books_line(summary: &RunSummary) -> String {
    match summary.books {
        None => "never marked".to_owned(),
        Some(books) => format!(
            "equity {:.2} vs hold {:.2} ({:+.2}%), fees {:.2} (+{:.2} unclaimed), costs {:.2} orders + {:.2} swaps, worst drawdown {:.2}%",
            books.equity,
            books.hold_equity,
            percent(books.equity / books.hold_equity - 1.0),
            books.fees_claimed,
            books.unclaimed_fees,
            books.action_costs,
            books.swap_costs,
            percent(books.worst_drawdown),
        ),
    }
}

fn protection_line(summary: &RunSummary) -> String {
    match &summary.protection {
        None => "never tripped".to_owned(),
        Some(trip) => format!(
            "latched after {:.2} days ({})",
            trip.after_secs as f64 / SECONDS_PER_DAY,
            trip.cause
        ),
    }
}
