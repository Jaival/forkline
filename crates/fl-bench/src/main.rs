use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, bail};
use serde::Deserialize;

#[derive(Deserialize)]
struct Report {
    marks: BTreeMap<String, f64>,
}

#[derive(Deserialize)]
struct Budgets {
    cold_launch_to_home_ms: f64,
}

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
    let budgets: Budgets = serde_json::from_slice(
        &fs::read(&budgets_path).with_context(|| format!("read {}", budgets_path.display()))?,
    )?;
    let start = report
        .marks
        .get("process_main")
        .context("report is missing process_main mark")?;
    let painted = report
        .marks
        .get("home_painted")
        .context("report is missing home_painted mark")?;
    let elapsed = painted - start;
    if elapsed < 0.0 {
        bail!("home_painted mark precedes process_main");
    }
    println!(
        "cold launch to home: {elapsed:.1} ms (budget {:.1} ms)",
        budgets.cold_launch_to_home_ms
    );
    if elapsed > budgets.cold_launch_to_home_ms {
        bail!("cold launch to home missed its budget");
    }
    Ok(())
}
