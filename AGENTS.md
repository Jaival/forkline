## Agent skills

### Issue tracker

Issues live in GitHub Issues and are managed with `gh`. See `docs/agents/issue-tracker.md`.

### Triage labels

Use `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, and `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Use the single-context layout: root `CONTEXT.md` and `docs/adr/`. See `docs/agents/domain.md`.

## Checks

- GPUI app, all: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
- GPUI app, narrow: `cargo test -p fl-app` (or `-p <crate>`)
- Bench: `cargo build --release -p fl-app -p fl-bench`, then `target/release/fl-bench gpui`

Tauri app (until removed):

- All: `bun run check` (typecheck, lint, vitest, clippy with `-D warnings`, cargo test)
- Narrow: `bun run test` for the UI, `cargo test -p <crate>` for one crate
- Build: `bun run build:ui` for the UI. `bun run build` builds the Tauri app; it is slow, run it only when asked.
