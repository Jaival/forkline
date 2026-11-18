//! Budget checker.
//!
//! - `fl-bench gpui [--app <exe>] [--data-dir <dir>] [--partial]`: start the
//!   GPUI app in bench mode, time process start to Inbox painted (`launch_ms`)
//!   and GPUI-ready to Inbox painted (`app_startup_ms`), and check the
//!   report against `bench/budgets-gpui.json`. `--partial` reports budgets no
//!   scenario measures yet instead of failing on them.
//! - `fl-bench [report.json] [budgets.json]`: check a Tauri app report.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("gpui") {
        return gpui(&args[1..]);
    }
    let mut it = args.into_iter();
    let report_path = it.next().map_or_else(|| "bench/report.json".into(), PathBuf::from);
    let budgets_path = it.next().map_or_else(|| "bench/budgets.json".into(), PathBuf::from);
    if it.next().is_some() {
        bail!("usage: fl-bench [report.json] [budgets.json] | fl-bench gpui [--app <exe>] [--data-dir <dir>] [--partial]");
    }
    let report: Report = read_json(&report_path)?;
    // Informational only: includes WebView2 startup, so it has no budget.
    if let (Some(start), Some(painted)) = (report.marks.get("process_main"), report.marks.get("home_painted")) {
        println!("process start to home: {:.1} ms (not budgeted)", painted - start);
    }
    check(&report, &read_json(&budgets_path)?, false)
}

fn gpui(args: &[String]) -> anyhow::Result<()> {
    let mut app = PathBuf::from("target/release/fl-app.exe");
    let mut data_dir = PathBuf::from("fake-data");
    let mut partial = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--app" => app = it.next().context("--app needs a path")?.into(),
            "--data-dir" => data_dir = it.next().context("--data-dir needs a path")?.into(),
            "--partial" => partial = true,
            other => bail!("unknown argument: {other}"),
        }
    }
    let out = std::env::temp_dir().join(format!("forkline-bench-{}.json", std::process::id()));
    let _ = fs::remove_file(&out);

    let started = epoch_ms();
    let status = Command::new(&app)
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("--bench")
        .arg(&out)
        .status()
        .with_context(|| format!("start {} (build it with cargo build -p fl-app --release)", app.display()))?;
    if !status.success() {
        bail!("{} exited with {status}", app.display());
    }
    let mut report: Report = read_json(&out)?;
    let painted = *report.marks.get("inbox_painted").context("report is missing inbox_painted")?;
    let mut phases: Vec<_> = report.marks.iter().filter(|(_, t)| **t <= painted).collect();
    phases.sort_by(|a, b| a.1.total_cmp(b.1));
    let line: Vec<String> = phases.iter().map(|(k, t)| format!("{k} +{:.0}", *t - started)).collect();
    println!("launch phases (ms from spawn): {}", line.join(", "));
    report.durations.insert("launch_ms".into(), vec![painted - started]);
    // Forkline's own part: from GPUI calling our run callback to Inbox painted.
    let run = *report.marks.get("run_callback").context("report is missing run_callback")?;
    report.durations.insert("app_startup_ms".into(), vec![painted - run]);
    check(&report, &read_json(Path::new("bench/budgets-gpui.json"))?, partial)
}

/// Print every budget; fail on a miss, or on a missing value unless `partial`.
fn check(report: &Report, budgets: &BTreeMap<String, f64>, partial: bool) -> anyhow::Result<()> {
    let mut missed = Vec::new();
    for (key, &budget) in budgets {
        if RUST_TESTED.contains(&key.as_str()) {
            println!("{key}: checked by cargo tests");
            continue;
        }
        // `*_fps` budgets are minimums, the rest maximums.
        let is_min = key.ends_with("_fps");
        let rel = if is_min { "min" } else { "max" };
        let Some(&value) = report.durations.get(key).and_then(|v| v.first()) else {
            println!("{key}: not measured ({rel} {budget:.1})");
            if !partial {
                missed.push(key.as_str());
            }
            continue;
        };
        let ok = if is_min { value >= budget } else { value <= budget };
        println!("{key}: {value:.1} ({rel} {budget:.1}){}", if ok { "" } else { "  MISSED" });
        if !ok {
            missed.push(key.as_str());
        }
    }
    for (key, values) in &report.durations {
        if !budgets.contains_key(key)
            && let Some(value) = values.first()
        {
            println!("{key}: {value:.1} (no budget)");
        }
    }
    if !missed.is_empty() {
        bail!("missed budgets: {}", missed.join(", "));
    }
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

fn epoch_ms() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}
