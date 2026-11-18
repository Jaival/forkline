# Forkline

Forkline is a local-first GitHub client for Windows 11. It syncs GitHub data
into SQLite and reads from that local cache. The UI is GPUI (Zed's UI
framework) with gpui-component, in the same Rust process as the backend.

## Development

Requirements: Rust and Git for Windows on PATH (used for diffs).

```powershell
cargo run -p fl-app --release
```

The default database lives under `%LOCALAPPDATA%\Forkline`. Use a separate
directory for generated data:

```powershell
cargo run -p fl-fake --release -- --data-dir fake-data
cargo run -p fl-app --release -- --offline --data-dir fake-data
```

Generated databases and `fake-data/` are ignored by Git. `--offline` starts
the app without the sync engine.

Checks:

```powershell
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Bench (release, fake data):

```powershell
cargo build --release -p fl-app -p fl-bench
target/release/fl-bench gpui
```

Sign in from the footer, or set `FORKLINE_GITHUB_TOKEN` in dev. See
`docs/HANDOFF.md` for the current state.
