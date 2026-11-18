## Agent skills

### Issue tracker

Issues live in GitHub Issues and are managed with `gh`. See `docs/agents/issue-tracker.md`.

### Triage labels

Use `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, and `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Use the single-context layout: root `CONTEXT.md` and `docs/adr/`. See `docs/agents/domain.md`.

## Checks

- All: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
- Narrow: `cargo test -p fl-app` (or `-p <crate>`)
- Build: `cargo build --release -p fl-app` (binary `target/release/forkline.exe`)
- Bench: `cargo build --release -p fl-app -p fl-bench`, then `target/release/fl-bench gpui`
