//! Bench mode state. The UI runs a scripted scenario (ui/src/bench.ts) and
//! reports marks and durations; `fl-bench` reads the JSON file.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

pub struct BenchState {
    pub out: Option<PathBuf>,
    pub report: Mutex<BenchReport>,
}

/// Written to the `--bench` output file.
#[derive(Debug, Default, Serialize)]
pub struct BenchReport {
    /// Wall clock (unix ms) for named moments, e.g. `home_painted`.
    pub marks: BTreeMap<String, f64>,
    /// Samples in ms, e.g. `tab_switch`, `open_item`.
    pub durations: BTreeMap<String, Vec<f64>>,
}

pub fn epoch_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

impl BenchState {
    pub fn new(out: Option<PathBuf>) -> Self {
        let mut report = BenchReport::default();
        report.marks.insert("process_main".into(), epoch_ms());
        Self {
            out,
            report: Mutex::new(report),
        }
    }

    pub fn enabled(&self) -> bool {
        self.out.is_some()
    }
}
