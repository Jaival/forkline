use fl_types::{CmdError, CmdResult};
use tauri::{AppHandle, State};

use crate::bench::{BenchState, epoch_ms};

/// Record a mark (wall clock now) or, with `duration_ms`, one sample.
#[tauri::command]
#[specta::specta]
pub async fn bench_mark(
    bench: State<'_, BenchState>,
    name: String,
    duration_ms: Option<f64>,
) -> CmdResult<()> {
    let mut r = bench
        .report
        .lock()
        .map_err(|_| CmdError::Other("bench lock poisoned".into()))?;
    match duration_ms {
        Some(ms) => r.durations.entry(name).or_default().push(ms),
        None => {
            r.marks.insert(name, epoch_ms());
        }
    }
    Ok(())
}

/// Write the report to the `--bench` file and exit the app.
#[tauri::command]
#[specta::specta]
pub async fn bench_finish(app: AppHandle, bench: State<'_, BenchState>) -> CmdResult<()> {
    let Some(out) = &bench.out else {
        return Err(CmdError::Other("not in bench mode".into()));
    };
    let json = {
        let r = bench
            .report
            .lock()
            .map_err(|_| CmdError::Other("bench lock poisoned".into()))?;
        serde_json::to_string_pretty(&*r).map_err(|e| CmdError::Other(e.to_string()))?
    };
    std::fs::write(out, json)
        .map_err(|e| CmdError::Other(format!("write {}: {e}", out.display())))?;
    app.exit(0);
    Ok(())
}
