# Forkline

Forkline is a local-first GitHub client for Windows 11. Its Rust/Tauri backend
will sync GitHub data into SQLite so the interface can read from a local cache.

## Development

Requirements: Rust, Bun, Git, and the Windows WebView2 runtime.

```powershell
bun install
bun run dev
```

The default database lives under `%LOCALAPPDATA%\Forkline`. Use a separate
directory for generated data, for example `bun run fake -- --data-dir fake-data`;
generated databases and `fake-data/` are ignored by Git.

Useful commands:

```powershell
bun run typecheck
bun run lint
bun run test
cargo test --workspace
bun run bindings
```

For a local fixture launch, seed the isolated database with `bun run fake`, then
start the app with `--data-dir fake-data`. The benchmark report produced by
`--bench <report.json>` can be checked with
`cargo run -p fl-bench --release -- <report.json> bench/budgets.json`.

Phase 0 establishes the workspace, SQLite schema, typed Tauri command boundary,
and UI shell. GitHub sync and feature views are added in later phases.
