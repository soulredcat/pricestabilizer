//! End-to-end runs of the built-in scenarios through a full session.

use std::path::Path;

use super::replay;
use crate::config::engine_config::EngineConfig;
use crate::market::feed::stream::open_feeds;
use crate::report::run_summary::RunSummary;
use crate::report::text_report;

const EXAMPLE: &str = include_str!("../../config/simulation.example.toml");
const START: u64 = 1_767_225_600;
const HOUR: u64 = 3_600;

fn run_with(config_text: &str, name: &str) -> RunSummary {
    let config = EngineConfig::parse(config_text, Path::new("example")).expect("valid config");
    let feed = open_feeds(&config.feed)
        .expect("scenario feeds open")
        .into_iter()
        .find(|feed| feed.name == name)
        .expect("scenario is listed in the example config");
    replay(&config, feed).expect("scenario replays").0
}

fn run_scenario(name: &str) -> RunSummary {
    run_with(EXAMPLE, name)
}

/// The example config plus an operator command file holding `commands`.
fn with_commands(file_name: &str, commands: &[String]) -> String {
    let path = std::env::temp_dir().join(file_name);
    std::fs::write(&path, commands.join("\n") + "\n").expect("writable");
    format!(
        "{EXAMPLE}\n[operator]\ncommands_path = {:?}\n",
        path.display().to_string()
    )
}

fn protection_cause(name: &str) -> Option<&'static str> {
    run_scenario(name).protection.map(|trip| trip.cause)
}

#[test]
fn emergencies_trip_protection_with_the_right_cause() {
    assert_eq!(protection_cause("flash_crash"), Some("extreme_move"));
    assert_eq!(protection_cause("stable_depeg"), Some("stable_depeg"));
    assert_eq!(protection_cause("liquidity_exit"), Some("depth_collapse"));
}

#[test]
fn calm_markets_never_trip_protection() {
    for name in ["steady_bull", "sideways", "liquidity_inflow"] {
        assert_eq!(protection_cause(name), None, "{name}");
    }
}

#[test]
fn books_always_reconcile_with_the_venue() {
    for name in ["steady_bull", "steady_bear", "pump_retrace", "data_gap"] {
        let cause = protection_cause(name);
        assert_ne!(cause, Some("ledger_mismatch"), "{name}");
        assert_ne!(cause, Some("venue_failures"), "{name}");
    }
}

#[test]
fn thin_market_never_gets_a_band() {
    let summary = run_scenario("low_liquidity");
    assert!(summary.latest_band.is_none());
    assert_eq!(summary.price_issues.get("thin_depth"), Some(&summary.ticks));
    assert!(summary.orders.is_empty());
}

#[test]
fn trending_market_moves_its_band_and_earns_fees() {
    let summary = run_scenario("steady_bull");
    assert!(
        summary
            .liquidity
            .get("reposition")
            .is_some_and(|count| *count > 0)
    );
    let books = summary.books.expect("books were marked");
    assert!(books.fees_claimed + books.unclaimed_fees > 0.0);
    assert!(books.action_costs > 0.0);
}

#[test]
fn feed_outage_is_flagged() {
    let summary = run_scenario("data_gap");
    assert_eq!(summary.price_issues.get("sample_gap"), Some(&1));
}

#[test]
fn treasury_proposes_but_never_buys_unapproved() {
    let summary = run_scenario("steady_bear");
    assert!(
        summary
            .events
            .get("treasury_proposed")
            .is_some_and(|count| *count > 0)
    );
    assert_eq!(summary.events.get("treasury_executed"), None);
    assert_eq!(summary.treasury_spent, 0.0);
}

#[test]
fn approved_proposal_is_bought_once() {
    let approvals: Vec<String> = (1..=5)
        .map(|hour| {
            format!(
                r#"{{"at": {}, "command": "approve", "proposal": 1}}"#,
                START + hour * HOUR
            )
        })
        .collect();
    let summary = run_with(
        &with_commands("pricestabilizer-approve.jsonl", &approvals),
        "steady_bear",
    );
    assert_eq!(summary.events.get("treasury_executed"), Some(&1));
    assert!((summary.treasury_spent - 250.0).abs() < 1e-9);
}

#[test]
fn operator_pause_and_resume_drive_protection() {
    let commands = [
        format!(r#"{{"at": {}, "command": "pause"}}"#, START + 24 * HOUR),
        format!(r#"{{"at": {}, "command": "resume"}}"#, START + 48 * HOUR),
    ];
    let summary = run_with(
        &with_commands("pricestabilizer-pause.jsonl", &commands),
        "sideways",
    );
    assert_eq!(
        summary.protection.map(|trip| trip.cause),
        Some("operator_pause")
    );
    assert_eq!(summary.events.get("operator_pause"), Some(&1));
    assert_eq!(summary.events.get("operator_resume"), Some(&1));
    let protected = summary.modes.get("protection").copied().unwrap_or(0);
    assert!(
        (280..=300).contains(&protected),
        "about one day paused: {protected}"
    );
}

#[test]
fn report_names_the_feed_protection_and_books() {
    let rendered = text_report::render(&run_scenario("flash_crash"));
    assert!(rendered.starts_with("== flash_crash =="));
    assert!(rendered.contains("latched after"));
    assert!(rendered.contains("equity"));
}

/// A recorded feed, an operator command file and a state directory, all
/// fresh, wired into the example config.
fn restartable_config(name: &str, commands: &str) -> (String, std::path::PathBuf) {
    let directory = std::env::temp_dir().join(format!("pricestabilizer-restart-{name}"));
    std::fs::remove_dir_all(&directory).ok();
    std::fs::create_dir_all(&directory).expect("temp dir is writable");
    let feed = directory.join("feed.jsonl");
    let commands_path = directory.join("commands.jsonl");
    std::fs::write(&commands_path, commands).expect("writable");
    let start = EXAMPLE.find("[feed]").expect("feed section");
    let end = EXAMPLE.find("[report]").expect("report section");
    let text = format!(
        "{}[feed]\nsource = \"jsonl\"\npath = {:?}\n\n[operator]\ncommands_path = {:?}\n\n[state]\ndir = {:?}\n",
        &EXAMPLE[..start],
        feed.display().to_string(),
        commands_path.display().to_string(),
        directory.join("state").display().to_string(),
    ) + &EXAMPLE[end..].replace("ledger_dir = \"target/ledgers\"", "");
    (text, feed)
}

fn write_snapshots(feed: &std::path::Path, count: u64) {
    let lines: String = (0..count)
        .map(|step| {
            format!(
                "{{\"timestamp\": {}, \"spot_price\": 1.0, \"quote_depth\": 50000.0, \"quote_volume\": 200.0, \"quote_usd_price\": 1.0}}\n",
                START + step * 300
            )
        })
        .collect();
    std::fs::write(feed, lines).expect("writable");
}

fn replay_file(config_text: &str) -> Result<RunSummary, crate::app::error::AppError> {
    let config = EngineConfig::parse(config_text, Path::new("restart")).expect("valid config");
    let feed = open_feeds(&config.feed)?.pop().expect("one feed");
    replay(&config, feed).map(|(summary, _)| summary)
}

#[test]
fn protection_and_consumed_commands_survive_a_restart() {
    let pause = format!(r#"{{"at": {}, "command": "pause"}}"#, START + 5 * 300) + "\n";
    let (text, feed) = restartable_config("latch", &pause);
    write_snapshots(&feed, 20);
    let first = replay_file(&text).expect("first run");
    assert_eq!(
        first.protection.map(|trip| trip.cause),
        Some("operator_pause")
    );

    // The feed grows and the engine restarts on the same state.
    write_snapshots(&feed, 40);
    let second = replay_file(&text).expect("second run");
    assert_eq!(second.ticks, 20, "already handled snapshots are refused");
    assert_eq!(second.skipped.get("out_of_order"), Some(&20));
    assert_eq!(
        second.events.get("operator_pause"),
        None,
        "never applied twice"
    );
    assert_eq!(
        second.modes.get("protection"),
        Some(&20),
        "the latch survived"
    );
}

#[test]
fn an_unreadable_state_file_refuses_to_start() {
    let (text, feed) = restartable_config("corrupt", "");
    write_snapshots(&feed, 3);
    let state_dir = feed.parent().expect("directory").join("state");
    std::fs::create_dir_all(&state_dir).expect("writable");
    let state_file = crate::state::state_file::path_for(&state_dir, &feed.display().to_string());
    std::fs::write(&state_file, "{ torn").expect("writable");
    assert!(matches!(
        replay_file(&text),
        Err(crate::app::error::AppError::StateParse { .. })
    ));
}

#[test]
fn paper_trading_resumes_after_a_restart_without_a_mismatch() {
    let (text, feed) = restartable_config("paper", "");
    write_snapshots(&feed, 30);
    let first = replay_file(&text).expect("first run");
    assert!(first.orders.contains_key("add_band"), "a band was opened");
    write_snapshots(&feed, 60);
    let second = replay_file(&text).expect("second run");
    assert_eq!(second.ticks, 30);
    assert_eq!(second.protection.map(|trip| trip.cause), None);
}

#[test]
fn an_unprofitable_market_gets_no_band_when_the_gate_is_on() {
    let gated = EXAMPLE.replace("min_fee_to_lvr = 0.0", "min_fee_to_lvr = 1000.0");
    let summary = run_with(&gated, "sideways");
    assert!(summary.latest_band.is_none());
    assert!(summary.liquidity.contains_key("hold_unprofitable"));
    assert!(summary.fee_to_lvr_ticks > 0);
}
