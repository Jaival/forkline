//! Forkline Tauri app: command registration, state, events, bench mode.

pub mod bench;
pub mod commands;
pub mod events;

use std::path::PathBuf;
use std::sync::Arc;

use fl_db::Db;
use tauri_specta::{Builder, ErrorHandlingMode, collect_commands, collect_events};

/// Shared state for every command.
pub struct AppState {
    pub db: Arc<Db>,
    pub data_dir: PathBuf,
}

/// Command-line flags (validated once here).
#[derive(Debug, Default)]
pub struct Args {
    pub data_dir: Option<PathBuf>,
    /// `--bench <out.json>`: run the scripted UI scenario, write timings, exit.
    pub bench_out: Option<PathBuf>,
    /// `--export-bindings`: write `ui/src/bindings.ts` and exit (debug builds only).
    pub export_bindings: bool,
}

impl Args {
    pub fn parse(mut it: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut a = Self::default();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--data-dir" => {
                    a.data_dir = Some(it.next().ok_or("--data-dir needs a path")?.into())
                }
                "--bench" => {
                    a.bench_out = Some(it.next().ok_or("--bench needs an output path")?.into())
                }
                "--export-bindings" => a.export_bindings = true,
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(a)
    }
}

/// All commands and events. One line per command; keep grouped by module.
pub fn specta_builder() -> Builder<tauri::Wry> {
    use commands::{actions, diff, inbox, issue, layout, lists, pr, repo, search};
    Builder::<tauri::Wry>::new()
        .dangerously_cast_bigints_to_number()
        .disable_serde_phases()
        .error_handling(ErrorHandlingMode::Throw)
        .commands(collect_commands![
            // layout (phase 0)
            layout::app_boot,
            layout::layout_save,
            layout::setting_get,
            layout::setting_set,
            // bench (phase 0)
            commands::bench::bench_mark,
            commands::bench::bench_finish,
            // repo (agent 2)
            repo::repos_pinned,
            repo::repo_overview,
            // lists + inbox + issue (agent 3)
            lists::home_feed,
            lists::issue_list,
            inbox::inbox_list,
            issue::issue_get,
            // pr + diff (agent 4)
            pr::pr_get,
            diff::diff_get,
            // search (agent 5)
            search::search,
            // sync + actions (agent 1)
            actions::sync_status,
            actions::action_enqueue,
        ])
        .events(collect_events![events::DataChangedEvent])
}

/// Bindings are a dev artifact: release builds never reach this code, so the
/// TypeScript exporter is dropped from the shipped binary.
#[cfg(debug_assertions)]
pub fn export_bindings(builder: &Builder<tauri::Wry>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/src/bindings.ts");
    builder
        .export(
            specta_typescript::Typescript::default()
                .header("// @ts-nocheck\n/* eslint-disable */"),
            path,
        )
        .expect("failed to export TypeScript bindings");
}

pub fn run() {
    let process_started_ms = bench::epoch_ms();
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("forkline: {e}");
            std::process::exit(2);
        }
    };
    let builder = specta_builder();
    #[cfg(debug_assertions)]
    {
        export_bindings(&builder);
        if args.export_bindings {
            return;
        }
    }
    #[cfg(not(debug_assertions))]
    if args.export_bindings {
        eprintln!("forkline: --export-bindings needs a debug build");
        std::process::exit(2);
    }

    let data_dir = args.data_dir.unwrap_or_else(fl_db::default_data_dir);
    std::fs::create_dir_all(&data_dir).expect("cannot create data directory");
    let db = Db::open(&fl_db::db_path(&data_dir)).expect("cannot open database");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            db: Arc::new(db),
            data_dir,
        })
        .manage(bench::BenchState::new(args.bench_out, process_started_ms))
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Forkline");
}
