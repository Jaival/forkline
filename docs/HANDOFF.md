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
database owner. Migration `0002_search_docs_inbox.sql` replaces the search table
and adds the global inbox index.

- Search rows: insert into `search_docs` (`comment_id = 0` for the issue's own
  title + body) and use its `id` as the `search_fts` rowid. Deleting a
  `search_docs` row (or its issue / repo) removes the FTS row through a trigger.
  To re-index an issue, delete its `search_docs` rows and insert fresh ones.
- Unread: an item is unread when `updated_at > COALESCE(last_viewed_at, 0)`.
  Sync leaves `last_viewed_at` NULL; opening the item sets it.
- Optimistic fields: `pending_fields` holds at most one pending action per
  `(table, row, field)`. A new action on a field that already has one supersedes
  it: keep the existing `old_value_json` (the last value GitHub confirmed),
  point `action_id` at the new action, and drop the old action from the queue if
  it has not been sent and touches no other field. On final failure, restore
  `old_value_json`.

## Performance targets

Targets are recorded in `bench/budgets.json`. The real-app harness is launched
with `forkline.exe --bench <output.json>` and reports UI timings. `fl-bench`
consumes that report and owns the budget comparison.

The launch budget (`page_start_to_home_ms`) runs from the page's navigation
start to Home rows painted. It does not include process start: Tauri and
WebView2 take about 430 ms before the page begins on the reference machine, and
app code cannot change that (WebView2 browser flags were tried; no effect).
`fl-bench` still prints the process-start number for reference.

Bench runs start from the
default layout, never save it, and wait for `signalReady(key)` (see
`ui/src/lib/perf.ts`) before a mark; every measured view must call it once its
rows are in the DOM. Feature phases should add
their query and interaction measurements as the relevant UI becomes available.

## Current scaffold boundary

The local shell and typed IPC are wired. GitHub synchronization, action retries,
search behavior, git operations, full feature views, and comprehensive UI
benchmark scenarios remain later-phase work.
