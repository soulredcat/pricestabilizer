//! End-to-end runs of the compiled binary: live snapshots on stdin, and the
//! connector protocol checked against the in-process paper venue.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const EXAMPLE: &str = include_str!("../config/simulation.example.toml");
const BINARY: &str = env!("CARGO_BIN_EXE_pricestabilizer");

/// The example config, shortened, without ledger files, and with the given
/// TOML section (header included) swapped for `replacement`.
fn config_with(section: &str, replacement: &str) -> String {
    let shortened = EXAMPLE
        .replace("steps = 2016", "steps = 600")
        .replace("ledger_dir = \"target/ledgers\"", "");
    let mut text = String::new();
    let mut skipping = false;
    for line in shortened.lines() {
        if line.starts_with('[') {
            skipping = line == section;
            if skipping {
                text.push_str(replacement);
                text.push('\n');
            }
        }
        if !skipping {
            text.push_str(line);
            text.push('\n');
        }
    }
    text
}

fn write_config(file_name: &str, text: &str) -> PathBuf {
    let path = std::env::temp_dir().join(file_name);
    fs::write(&path, text).expect("temp dir is writable");
    path
}

fn run(config: &PathBuf, stdin: &str) -> String {
    let mut child = Command::new(BINARY)
        .arg(config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("binary starts");
    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(stdin.as_bytes())
        .expect("stdin accepts input");
    let output = child.wait_with_output().expect("binary finishes");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 output")
}

#[test]
fn paper_connector_over_the_protocol_matches_the_in_process_venue() {
    let paper = write_config("pricestabilizer-e2e-paper.toml", &config_with("[none]", ""));
    // A venue that sends orders must persist its state; start from none.
    let state_dir = std::env::temp_dir().join("pricestabilizer-e2e-state");
    fs::remove_dir_all(&state_dir).ok();
    let connector = format!(
        "[venue]\nkind = \"external\"\ncommand = [{:?}, \"paper-connector\", {:?}]\ntimeout_secs = 30\nsend_orders = true\n\n[state]\ndir = {:?}",
        BINARY,
        paper.display().to_string(),
        state_dir.display().to_string()
    );
    let external = write_config(
        "pricestabilizer-e2e-external.toml",
        &config_with("[venue]", &connector),
    );
    let in_process = run(&paper, "");
    assert!(in_process.contains("== flash_crash =="));
    assert_eq!(run(&external, ""), in_process);
}

#[test]
fn snapshots_piped_on_stdin_are_traded_live() {
    let config = write_config(
        "pricestabilizer-e2e-stdin.toml",
        &config_with(
            "[feed]",
            "[feed]\nsource = \"stdin\"\nmax_snapshot_age_secs = 600",
        ),
    );
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after 1970")
        .as_secs();
    let line = |timestamp: u64| {
        format!(
            "{{\"timestamp\": {timestamp}, \"spot_price\": 1.0, \"quote_depth\": 50000.0, \"quote_volume\": 200.0, \"quote_usd_price\": 1.0}}\n"
        )
    };
    // Three live snapshots, one stale and one garbled line: the last two are
    // skipped and counted, never acted on.
    let snapshots = [
        line(now - 120),
        line(now - 60),
        line(now - 86_400),
        "{not json}\n".to_owned(),
        line(now),
    ]
    .concat();
    let output = run(&config, &snapshots);
    assert!(output.starts_with("== stdin == 3 ticks"), "{output}");
    assert!(
        output.contains("skipped      invalid_json 1, out_of_order 1"),
        "{output}"
    );
}
