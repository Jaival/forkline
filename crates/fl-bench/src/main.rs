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

#[derive(Deserialize)]
struct Budgets {
    /// Page navigation start to Home rows painted. Excludes Tauri / WebView2
    /// process startup, which the app cannot speed up.
    page_start_to_home_ms: f64,
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
    let page_to_home = report
        .durations
        .get("page_start_to_home")
        .and_then(|v| v.first())
        .context("report is missing page_start_to_home")?;

    // Informational only: includes WebView2 startup, so it has no budget.
    let start = report.marks.get("process_main");
    let painted = report.marks.get("home_painted");
    if let (Some(start), Some(painted)) = (start, painted) {
        println!("process start to home: {:.1} ms (not budgeted)", painted - start);
    }

    println!(
        "page start to home: {page_to_home:.1} ms (budget {:.1} ms)",
        budgets.page_start_to_home_ms
    );
    if *page_to_home > budgets.page_start_to_home_ms {
        bail!("page start to home missed its budget");
    }
    Ok(())
}
