use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, bail};
use serde::Deserialize;

#[derive(Deserialize)]
struct Report {
    marks: BTreeMap<String, f64>,
    durations: BTreeMap<String, Vec<f64>>,
}

/// Checked by `fl-sync`'s queue tests (enqueue never waits on the network),
/// not by the UI scenario.
const RUST_TESTED: &[&str] = &["optimistic_action_network_wait_ms"];

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let report_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| "bench/report.json".into());
    let budgets_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| "bench/budgets.json".into());
    if args.next().is_some() {
        bail!("usage: fl-bench [report.json] [budgets.json]");
    }

    let report: Report = serde_json::from_slice(
        &fs::read(&report_path).with_context(|| format!("read {}", report_path.display()))?,
    )?;
    // Key = report duration name. `*_fps` budgets are minimums, the rest maximums.
    let budgets: BTreeMap<String, f64> = serde_json::from_slice(
        &fs::read(&budgets_path).with_context(|| format!("read {}", budgets_path.display()))?,
    )?;

    // Informational only: includes WebView2 startup, so it has no budget.
    let start = report.marks.get("process_main");
    let painted = report.marks.get("home_painted");
    if let (Some(start), Some(painted)) = (start, painted) {
        println!("process start to home: {:.1} ms (not budgeted)", painted - start);
    }

    let mut missed = Vec::new();
    for (key, &budget) in &budgets {
        if RUST_TESTED.contains(&key.as_str()) {
            println!("{key}: checked by cargo tests");
            continue;
        }
        let value = *report
            .durations
            .get(key)
            .and_then(|v| v.first())
            .with_context(|| format!("report is missing {key}"))?;
        let is_min = key.ends_with("_fps");
        let ok = if is_min { value >= budget } else { value <= budget };
        let rel = if is_min { "min" } else { "max" };
        println!(
            "{key}: {value:.1} ({rel} {budget:.1}){}",
            if ok { "" } else { "  MISSED" }
        );
        if !ok {
            missed.push(key.as_str());
        }
    }
    if !missed.is_empty() {
        bail!("missed budgets: {}", missed.join(", "));
    }
    Ok(())
}
