# Phase 0 handoff

## Workspace ownership

- `crates/fl-types`: shared DTOs and command errors.
- `crates/fl-db`: SQLite connection setup, migration 1, query modules.
- `crates/fl-sync`, `crates/fl-git`, `crates/fl-search`: Phase 1 service boundaries.
- `crates/fl-fake`: isolated deterministic fixture data generator.
- `crates/fl-bench`: budget checker and local query benchmarks.
- `app`: Tauri startup, typed command registration, and event boundary.
- `ui`: React shell, generated Tauri bindings, and client-side layout state.

## Shared rules

- Keep the approved stack fixed: Tauri 2, Rust, SQLite/FTS5, Bun, React,
  TypeScript, Vite, Tailwind, TanStack Query, Vitest, and TanStack Virtual.
- Keep each feature's backend implementation in its matching command/query
  module. Register commands centrally in `app/src/lib.rs`.
- Regenerate `ui/src/bindings.ts` with `bun run bindings`; do not hand edit it.
- Keep fake fixtures and all SQLite database files out of Git. Use `--data-dir`
  to isolate fixture databases from the real user database.
- UI commands must page collections and return no more than 100 rows per call.
- Emit `data-changed` scopes after writes so the UI can invalidate cached queries.

## Schema

Migration `crates/fl-db/migrations/0001_init.sql` is the contract for repositories,
issues/PRs, comments, reviews, checks, notifications, optimistic action state,
settings, layout persistence, and FTS5. Keep migration changes additive and
versioned; do not edit an already shipped migration without coordinating the
database owner.

## Performance targets

Targets are recorded in `bench/budgets.json`. The real-app harness is launched
with `forkline.exe --bench <output.json>` and reports UI timings. `fl-bench`
consumes that report and owns the budget comparison; feature phases should add
their query and interaction measurements as the relevant UI becomes available.

## Current scaffold boundary

The local shell and typed IPC are wired. GitHub synchronization, action retries,
search behavior, git operations, full feature views, and comprehensive UI
benchmark scenarios remain later-phase work.
