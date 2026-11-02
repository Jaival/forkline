# Handoff (Phase 0 + Phase 1)

## Workspace ownership

- `crates/fl-types`: shared DTOs and command errors.
- `crates/fl-db`: SQLite connection setup, migration 1, query modules.
- `crates/fl-sync`: GitHub auth (device flow, Windows Credential Manager),
  REST/GraphQL client, sync engine, optimistic action queue.
- `crates/fl-git`: `GitStore` over the `git` CLI (bare blobless clones, diffs).
- `crates/fl-search`: FTS5 indexing (`index_issue`) and query parser (`search`).
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

## Phase 1 state

Plan and per-area ownership: `docs/PHASE1.md`. Five agents built the areas in
parallel; all are merged into `phase-1`.

Works (tests + bench on fake data; not yet exercised against real GitHub):

- Sync: repo metadata + `starred` for all repos, deep sync for pinned repos
  (newest 300 issues / 60 PRs on first sync), notifications polling, rate
  limit pauses. Engine starts in `setup`; not in bench mode.
- Actions: every `fl_types::Action` is applied locally at once, sent by a
  worker, retried with backoff, rolled back on final failure. Failed actions
  show in the footer with Retry / Discard.
- Views: Home, Inbox, PR / Issue / Review-request lists, issue detail, PR
  view (conversation, commits, checks, files + diff), repo overview,
  Repositories, Starred, Search, sign-in dialog, footer.
- Keys: `Ctrl+K` palette, `Ctrl+Tab`, `/`, `j` / `k` / Enter in lists
  (`VirtualList` takes `onOpen` and wires `listNav`).
- Bench: all budgets in `bench/budgets.json` are checked by `fl-bench`.
  Release, 10k fake issues: launch 178-200 ms, open item 14-19 ms, tab
  switch 4 ms, search p95 17-19 ms, diff scroll 240 fps.

Rules added in Phase 1:

- Migrations 0003-0006 are numbered by schema version. Delete dev and fake
  databases created before the renumbering.
- Notifications in `action_queue` / `pending_fields` use
  `CAST(notifications.id AS INTEGER)` as the row id.
- Local optimistic rows have negative ids. Sync deletes local rows that no
  action owns any more (`confirmed_local` in `fl-sync/src/sync.rs`).
- `data-changed` scopes: `issues`, `repos`, `inbox`, `pr`, `sync`, `search`.
- Git runs with credential helpers disabled and the token only in
  `GIT_CONFIG_*` env vars. Git for Windows is required.

## Next

1. Register the GitHub OAuth App (device flow on) and build with
   `FORKLINE_GITHUB_CLIENT_ID`. Until then use `FORKLINE_GITHUB_TOKEN`.
2. Manual pass in the running app against a real account: sign-in, first
   sync, every view, each action kind, diff of a real PR, failed action
   rollback. No view has been checked by eye yet.
3. Known gaps:
   - Comments, timeline, reviews, checks, commits stop at 100 per item.
   - Not synced: contributors, star history, timeline events.
   - `reviews` has no `body_md`; local reviews store escaped text in `body_html`.
   - No shared UI handler for `CmdError::Unauthorized` (footer covers sign-in).
   - Diff: no multi-line comments, long lines clip, no `j` / `k` over files.
   - Search: all terms must match in one doc (title or one comment).
   - Repo tab titles show the id; pin toggle is not optimistic.
