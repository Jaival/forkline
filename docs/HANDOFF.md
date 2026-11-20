# Handoff (Phase 0 + Phase 1, GPUI G0)

## Workspace ownership

- `crates/fl-types`: shared DTOs and command errors.
- `crates/fl-db`: SQLite connection setup, migration 1, query modules.
- `crates/fl-sync`: GitHub auth (token order: `FORKLINE_GITHUB_TOKEN`, saved
  token in Windows Credential Manager, `gh auth token`; device flow from the
  sign-in dialog), REST/GraphQL client, sync engine, optimistic action queue.
- `crates/fl-git`: `GitStore` over the `git` CLI (bare blobless clones, diffs).
- `crates/fl-search`: FTS5 indexing (`index_issue`) and query parser (`search`).
- `crates/fl-fake`: isolated deterministic fixture data generator.
- `crates/fl-bench`: budget checker (`fl-bench gpui` for the GPUI app).
- `crates/fl-app`: the GPUI app (binary `forkline`).

## Shared rules

- The UI is GPUI + gpui-component; see `docs/adr/0001-gpui.md` for the
  stack, pins and changed decisions. The Tauri + React app (`app/`, `ui/`)
  was deleted on 2026-11-18. Do not change the stack without asking.
- Keep each feature's backend implementation in its matching query module.
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

- Migrations 0003-0007 are numbered by schema version. Delete dev and fake
  databases created before the renumbering.
- Notifications in `action_queue` / `pending_fields` use
  `CAST(notifications.id AS INTEGER)` as the row id.
- Local optimistic rows have negative ids. Sync deletes local rows that no
  action owns any more (`confirmed_local` in `fl-sync/src/sync.rs`).
- `data-changed` scopes: `issues`, `repos`, `inbox`, `pr`, `sync`, `search`.
- `CmdError::Unauthorized`: `noteError` in `ui/src/lib/query.ts` runs for every
  query and mutation error (and in `useAction` / the footer). It refetches
  `sync` and reopens the sign-in dialog. A git 401 maps to it through
  `GitError::Unauthorized`. The engine emits `sync` when GitHub rejects the token.
- Git runs with credential helpers disabled and the token only in
  `GIT_CONFIG_*` env vars. Git for Windows is required.

Delete dev and fake DBs: migration 0007 (`reviews.body_md`).

## Next

1. Register the GitHub OAuth App (device flow on) and build with
   `FORKLINE_GITHUB_CLIENT_ID`. Until then use `FORKLINE_GITHUB_TOKEN`.
2. Manual pass in the running app against a real account: sign-in, first
   sync, every view, each action kind, diff of a real PR, failed action
   rollback. No view has been checked by eye yet.
3. Known gaps:
   - Connections longer than 1100 nodes (comments, timeline, reviews, commits, checks,
     thread comments) stay partial: `MAX_EXTRA_PAGES` = 10 extra pages per item.
     Labels and assignees are not paged.
   - Star history starts at the first sync after 0007; no backfill (GitHub has no API).
     Timeline event ids are assigned by SQLite (GraphQL has no numeric id).
   - Diff: a Shift+click range that spans two hunks is sent as is; GitHub
     rejects it and the action rolls back. Changing the range after typing
     clears the comment box.
   - Search: broad queries (> 2000 docs) rank by recency and take the snippet
     from the first doc matching any term. A phrase cannot span title and a
     comment.

## GPUI G0 state

Decision record: `docs/adr/0001-gpui.md`. Branch: `gpui`.

Done:

- `fl-app` on `gpui-kit =0.9.0` (pins `gpui-pre =0.3.11`). Window, theme
  (system / light / dark, setting `theme`, `ToggleTheme` action), sidebar
  (nav + pinned repos with open PR / issue counts), dock with tabs and split
  panes, footer with sync status. Layout saved under settings key
  `gpui_layout` (not `ui_layout`, the removed Tauri app's row).
- Keys: `Ctrl+T` new tab, `Ctrl+W` close tab, `Ctrl+\` split right.
  Covered by UI tests in `crates/fl-app/src/workspace.rs`.
- `Store` global (`crates/fl-app/src/store.rs`): `read` runs SQLite on a
  background thread, `run` runs engine futures on the tokio runtime, and the
  store emits `DataChanged { scopes }` for sync and action changes.
- Migration 0008: `following`, `explore_stars`, `repos.explore_only`. The
  Repositories list hides explore-only repos.
- `fl-fake`: 50 repos, 20,000 issues and PRs, about 97k comments, Explore
  rows, and a real bare repo `git/forkline-demo/project-001.git` with five PRs
  (#3, #6, #9, #12, #15 in repo 1; #12 changes a 10,000-line file).
- Bench: `cargo build --release -p fl-app -p fl-bench`, then
  `target/release/fl-bench gpui [--partial]`. It spawns the app, so
  `launch_ms` runs from process start to Home painted.

Seams for G1:

- A view: add a module under `crates/fl-app/src/views/` and an arm in
  `views::build`. Call `bench::ready("<key>", window, cx)` once its rows are
  on screen.
- Open a tab: `Workspace::open(ViewRef, window, cx)` selects an open tab or
  adds one to the focused pane. Add `ViewRef` variants; never rename them
  (saved layouts hold them).
- Icons: add Lucide names to `ExtraIcons` in `crates/fl-app/src/assets.rs`.
- Bench scenarios: add steps to `bench::run` and write the budget key.

Bench on the reference machine (RTX 4060 laptop, release, warm):
`switch_tab_ms` 4.1, `launch_ms` 410-560 (budget 450), `app_startup_ms`
about 45 (budget 60). About 330 ms of launch is GPUI platform start
(DirectWrite 150, DXGI/driver 155, D3D11 device 145) before any Forkline code;
see ADR 0001 for why the budget is 450, not 200. A cold first run after boot
can exceed 450; run the bench twice.

## GPUI G1 state

Plan: `docs/design/ui-plan.md` (7 steps). Mockups: `docs/design/ui-mockups.html`
(B = shell, D = PR Files tab). Steps 1-6 are done; step 7's views are done, the Tauri
removal waits for the parity gaps. PRs: #2 (G0 + steps 1-3, `gpui`), #3 (step 4,
`gpui-step4`), #4 (step 5, `gpui-step5`), #5 (step 6, `gpui-step6`), step 7 on
`gpui-step7` (stacked on `gpui-step6`).

Done (steps 1 and 2):

- Shell: icon rail (`crates/fl-app/src/rail.rs`) replaces the sidebar. Inbox,
  Pull requests, Issues, Reviews, Repositories, Starred, Explore, Search, a
  pinned-repositories menu; theme toggle and Settings at the bottom. The Inbox
  icon shows a dot when a notification is unread. Element ids `rail-*`.
- Inbox is the launch view: the default layout and `Ctrl+T` open
  `ViewRef::Inbox`. `ViewRef::Home` stays; `ViewRef::Settings` is new (still a
  placeholder). `LAYOUT_VERSION` is 2: a layout saved by G0 is dropped once,
  so it cannot open on the Home placeholder.
- Queue list (`views/queue.rs`): one grouped, virtualized list for Inbox,
  Pulls, Issues and Reviews. `j` / `down`, `k` / `up`, `Enter` (key context
  `Queue`); click selects, double-click opens. Rows: state icon, title, repo,
  number, author, checks, age, unread mark. Header: title, total, repo filter
  (pinned repos). First page is read synchronously (launch budget); later
  pages load on scroll; a `DataChanged` of `inbox`, `issues` or `pr` reloads
  and keeps the selection by key.
- `views/list_view.rs`: queue plus reading pane in one dock panel
  (resizable). `views/reading.rs` is a stub: header of the selected row only.
- Data: `fl_types::queue` (`QueueQuery`, `QueueGroup`, `QueueRow`,
  `CheckState`) and `fl_db::queries::queue` (`page`, `counts`), covered by
  `crates/fl-db/tests/queue.rs` and the query-plan test.
  `queries::inbox::unread_count` feeds the rail dot.
- Inbox groups: Review requested, Mentioned, Assigned, Your PRs, plus
  **Other** for every other notification reason (subscribed, comment, ...).
  The plan names four groups; Other was added so no notification is hidden.
- `fl-fake`: notification reasons now vary (`(id / 25) % 5`); before, every
  fake notification was `review_requested`. Regenerate fake data.
- `AGENTS.md` Checks lists the cargo commands.

Seams added:

- `views::build` returns `Built { view, focus }`. The panel focuses `focus`,
  so a view's key context receives keys. Bind view keys in `views::init`.
- A view opens a tab with `window.dispatch_action(Box::new(OpenView(view_ref)), cx)`
  (`workspace::OpenView`); it needs no workspace handle.
- Selection: `Queue` emits `QueueEvent::Selected(Option<QueueRow>)`;
  `ListView` passes it to `ReadingPane::set_item`.
- UI tests: `crate::test_support::open_seeded(cx, dir, sql)` opens a workspace
  window over a temp database. Tests press keys with `window.press`.
- `fl-app --offline`: no sync engine. Use it with `--data-dir` to look at fake
  data (without it the engine syncs real GitHub data into that folder).
- Bench mode ignores the saved layout (it used to load a G0 layout from the
  fake database, and then measured a Home placeholder). `bench::run` waits for
  `inbox_painted` before its first step. Marks: `inbox_painted`,
  `pulls_painted`, `issues_painted`, `reviews_painted`.

Bench (reference machine, release, warm, 3 runs, fake data):
`app_startup_ms` 41-53 (budget 60), `switch_tab_ms` 4.0-4.2 (budget 8),
`launch_ms` 497-513 (budget 600; raised from 450 on 2026-11-16, see ADR
0001). Launch is platform start: about 450 ms pass before GPUI calls Forkline
code (G0 measured 410-560). Not measured
yet: `search_p95_ms` (step 6),
`diff_scroll_fps` (step 5); run `fl-bench gpui --partial` until then.

Checked by eye (PrintWindow capture, dark theme): rail and Inbox queue at
launch. Not checked by eye: Pulls / Issues / Reviews, selection highlight,
menus, light theme. Keys are covered by UI tests only.

Done (step 3, reading pane, 2026-11-17):

- `views/reading.rs`: `ReadingPane` shows one issue or PR. Beside a queue it
  follows the selected row (header from the row at once, details read on a
  background thread); `ViewRef::Issue` / `ViewRef::Pr` tabs build the same
  view (`ReadingPane::build`). Key context `Reading` (no keys yet).
  - `reading/header.rs`: repo and number, title, state badge (Draft for draft
    PRs), PR `author -> base`, `+a -d`, check summary; issue author and
    comment count; label pills in the label colour.
  - `reading/conversation.rs`: description, comments, reviews (verb coloured)
    and, for issues, timeline events, oldest first, in a GPUI `list` with a
    scrollbar. Bodies through gpui-component `TextView` (markdown, else
    HTML). "N more comments on GitHub" when the cache holds fewer. A refresh
    of the same item keeps the scroll position.
  - `reading/tabs.rs`: PR tabs Files (changed files from git, `uniform_list`),
    Commits, Checks (failures first). An issue shows the conversation only. A
    PR without `pr_details` shows as its conversation.
- Opening an unread item sets `last_viewed_at` and emits `issues`. Selecting
  an unread Inbox row also enqueues `Action::NotificationRead` (marks it read
  on GitHub through the action queue). Nothing is marked at launch.
- `Store`: `emit_changed(scopes)` for local writes; `diff_summary(pr_id)`
  (offline: local clone only; online: `ensure_commits` first; replaced by
  `diff_patch` in step 5); `read_only`
  (bench runs skip the writes opening an item makes). The action emitter now
  runs offline too, so a queued action refreshes the views.
- `fl-app` depends on `fl-git`.
- Tests: `views/reading/tests.rs` (13 UI tests: details load, read marks,
  Enter opens a reading tab, missing item, stale reads, tab reset, Files
  failure, reload on `issues`), unit tests in `conversation.rs` and `tabs.rs`.
  Notification ids in test seeds must be numeric: the action queue parses them.
- Bench: step `open_cached_item_ms` (20 `SelectNext` in the Inbox, time to
  `item_painted`; samples on rows without an item are skipped). Marks
  `workspace_new` / `workspace_built` split the startup phases.

Bench (reference machine, release, fake data, 2026-11-17, `--partial`, 3 warm
runs): `open_cached_item_ms` 5.9-6.1 (budget 30), `switch_tab_ms` 4.1 (8),
`app_startup_ms` 39-47 (60), `launch_ms` 460-544 (600; a cold first run
took 632, 228 ms of it before `main`). Earlier the same day `app_startup_ms`
read 61-94 for several runs: the time sat between `store_ready` and
`workspace_new` (theme apply and GPUI window creation, no Forkline code;
Workspace itself takes 2-3 ms) and went back to 23-28 ms without a code
change. If it misses again, read those two marks first.

Checked by eye (PrintWindow, dark, `--offline` on a copy of fake data with a
saved layout holding Issue and PR tabs): PR #3 header, tabs, conversation
with markdown (inline code, quote, code block), review verbs; Files tab on
the demo git repo. Not checked by eye: queue + pane side by side after `j`
(keys cannot be sent; covered by UI tests), Commits / Checks tabs, light theme.

Done (step 4, action bar and keys, 2026-11-17):

- `views/reading/action_bar.rs`: a bar under the header with the buttons
  that apply to the item: Approve (open PR), Reply (any loaded item), Done and
  Snooze (Inbox rows only). Each button shows its key. An action error
  (e.g. the queue rejects it) shows in the bar. Element ids `action-*`,
  `reply-send`.
- Keys (`views::init`, context `(Queue || Reading) && !Input`): `a` approve,
  `r` reply, `e` done, `h` snooze. `ListView` forwards them from the queue to
  its pane, so they work while the queue holds the focus. `!Input`: the keys
  type text in the reply box.
  - `A`: `Action::SubmitReview { event: Approve }` through the action queue
    (local review row at once).
  - `R`: reply box (gpui-component `Textarea`) under the item, focused.
    `Ctrl+Enter` (or the Comment button) queues `Action::Comment`; `Esc`
    closes it. Focus returns to where `R` was pressed. Another item drops
    the draft.
  - `E` / `H`: local only, no action for GitHub. `E` sets `done_at`; `H` sets
    `snoozed_until` = now + 1 day. Emits `inbox`.
- Migration `0009_triage.sql`: `notifications.done_at`, `snoozed_until`.
  `fl_db::queries::inbox::IN_INBOX` is the one "in the inbox" condition
  (Inbox queue, Tauri inbox list, `unread_count`): not `done`, not updated
  only before `done_at`, snooze over. `inbox::set_done` stores
  `MAX(now, updated_at)` (fake data has future timestamps), and clears a
  snooze; `inbox::snooze`.
- Queue: when the selected row leaves on reload (Done, Snooze, sync), the row
  that took its place is selected, so triage goes on. That row then counts as
  viewed (and, unread, gets a `NotificationRead`), like any selection.
- `ReadingPane::new(inbox, cx)`: `inbox` turns on Done / Snooze.
- Tests: 7 UI tests in `views/reading/tests.rs` (approve, `a` on an issue,
  reply send with the action letters typed, `Esc`, done selects the next
  row, snooze for a day, no Done / Snooze in an item tab); 2 in
  `crates/fl-db/tests/queue.rs` (done returns on a later update, snooze ends).

Bench (2026-11-17, release, `--partial`, 5 warm runs, laptop ON BATTERY):
`open_cached_item_ms` 16.2-17.0 (30), `switch_tab_ms` 9.6-9.8 (8, MISSED),
`app_startup_ms` 43-52 (60), `launch_ms` 438-512 (600; the first, cold run
843). Not caused by step 4: an A/B run with the action bar turned off gave
the same numbers (16.4-16.6 / 9.7-9.8), and the tab switch path has no
step-4 code. Both numbers look like whole frames at a lower refresh rate on
battery. Re-run on AC power before step 5.

Checked by eye (PrintWindow, dark, `--offline` on `target/tmp/shot-data`):
PR tab shows Approve A, Reply R; no Done / Snooze outside Inbox. Not
checked by eye: the bar beside the Inbox queue, the reply box (keys cannot
be sent; both covered by UI tests).

Done (step 5, PR Files tab, 2026-11-17):

- `views/reading/files.rs`: `FilesTab` entity, the Review desk (mockup D).
  Built the first time the Files tab opens; dropped with the item.
  - Left: changed files (status letter, Viewed check, `+n`), header
    `Files · N of M viewed`. Click selects and scrolls to the file.
  - Centre: toolbar (current path, `[ ] file` hint, Unified / Split toggle,
    Viewed `V` button) and gpui-component `Diff` over one `DiffState` holding
    every file of the PR (one virtualized list). Each file header has a
    Viewed button. Lines with an open review thread are tinted.
  - Right rail (240 px): review threads (open first; up to 3 comments each,
    plain text; click scrolls to and selects the lines; outdated threads do
    not scroll) and the head commit's checks.
- Data: the whole patch from git (`GitStore::diff_patch`, `-M -U3`, same
  range as `diff_summary`), parsed with `DiffFile::parse` on a background
  thread. `Store::diff_patch` replaced `Store::diff_summary` (shared
  `resolve_clone`). Threads: `pr::threads` (first 100; "More threads on
  GitHub" past that). Viewed: existing `pr_viewed_files` (`pr::viewed_paths`,
  `pr::set_viewed`), so no migration. Resets when the head commit changes.
- Keys (context `(Queue || Reading) && !Input`, act only while the Files tab
  is on show; `ListView` forwards them): `]` / `[` next / previous file
  (wraps), `v` toggles Viewed on the current file. Marking viewed collapses
  the file and moves to the next file not viewed. A failed write reverts.
  Action `ShowFiles` opens the Files tab (no key yet; the bench uses it).
  The `Diff` keeps its own keys (arrows, PageUp/Down, Home/End, Ctrl+C) once
  clicked.
- Tests: 6 UI tests in `views/reading/files/tests.rs` over a real git repo
  built in the temp dir (load, `]` / `[` wrap, `v` on / off with the DB row,
  all viewed, Viewed persists into a fresh tab, keys inert outside Files).
  `fl-git`: `diff_patch` test in `crates/fl-git/tests/git.rs`.
- Bench: step `diff_scroll_fps`. Opens the fake PR `tune-generated`
  (`src/generated/big.rs`, 10k lines, every 10th changed: about 9k diff
  rows, 1000 hunks) in a PR tab, `ShowFiles`, waits for `diff_painted`,
  focuses the diff, then dispatches `diff::ScrollDown` (24 px) twice per
  frame for 2 s and counts painted frames. `bench::diff_shown` hands the
  `DiffState` to the bench.

Bench (2026-11-17, release, `--partial`, 6 warm runs, ON AC POWER, display
240 Hz): `diff_scroll_fps` 216-228 (min 120), `open_cached_item_ms` 6.0-6.6
(30), `switch_tab_ms` 3.9-4.0 (8), `launch_ms` 525-568 (600; first run 801
cold), `app_startup_ms` 49-55 in 4 runs, 61 and 89 in 2 (60). The misses sit
between `store_ready` and `workspace_new` (window creation, no Forkline
code), as noted in step 3. Step 4's battery numbers (9.7 / 16.5) are back to
4.0 / 6.2 on AC, so they were power, not code.

Checked by eye (PrintWindow during a bench run, dark, `--offline --bench`
on `target/tmp/shot-data`; image `target/tmp/g1s5-files-a.png`): PR #12
Files tab with file list, toolbar, hunks with +/- tint and inline change
emphasis, empty thread rail. Not checked by eye: Split mode, a collapsed
viewed file, threads in the rail (fake data has none on that PR), light
theme. Keys are covered by UI tests only.

Done (step 6, palette and query bar, 2026-11-17):

- `/` query bar in every queue (`views/queue.rs`): a row under the header
  with a gpui-component `Input`, a `Views` menu (saved views of this kind,
  Clear filter) and, while a filter is set, `N results · X ms` and `Save
  view`. Element ids `queue-*`.
  - Data: `fl_search::queue_filter(conn, query)` turns the query into a
    `fl_db::queries::queue::Cond`; `queue::page` / `counts` take
    `Option<&Cond>`. Same operators as search (`is:`, `author:`, `label:`,
    `repo:`, `assignee:`, `-` negation); every text term must hit the title,
    body or a comment (last term as a prefix). Queue order is unchanged
    (group, newest first), not rank. A term matching more than 2000 docs is
    checked per row (`EXISTS` over `search_docs`), else as an id set: mapping
    100k docs to issues was the cost (18 ms -> 6 ms for `fixture item`).
  - A filter change selects no row, reads the first page in the background,
    shows it, then reads the group counts in a second read (counts take up
    to 30 ms on 20k issues). Header numbers are blank until then. Reload,
    load-more and the repo filter keep the filter.
  - Keys: `/` (`FocusFilter`, context `(Queue || Reading) && !Input`; the
    `ListView` forwards it from the pane) focuses the input. In the input:
    `Esc` clears a filter and returns to the list, `Enter` returns to the
    list and keeps it. The list keys (`j` / `k` / arrows / `Enter`) are now
    bound in `Queue && !Input`, so typing `j` in the bar types.
  - Saved views: migration `0010_saved_views.sql` (`kind`, `name`, `query`,
    unique per kind and name); `fl_db::queries::views::{list, save,
    delete}`; `fl_types::SavedView`, `QueueKind::as_str/parse`. `SaveView`
    saves the filter with its text as name. Scope `views` in `DataChanged`.
- `Ctrl+K` palette (`palette.rs`, action `OpenPalette`): a gpui-component
  `Command` in an overlay over the workspace (`deferred`, backdrop click
  closes, `Ctrl+K` again closes). Groups: Actions (only those the focused
  view can run when it opens: Approve, Reply, Mark done, Snooze, files keys,
  Filter queue, Save filter as view), Go to (views and pinned repos'
  Pulls / Issues), Saved views (`View: …` opens the list and sets the
  filter; `Delete view: …`), Workspace (new / close tab, split, theme), then
  up to 8 `fl_search::search` hits for the typed text and `Search for "…"`.
  Own word filter (every word in the label). The palette never runs a
  command itself: the workspace closes it, gives focus back, then runs it,
  so actions reach the view. `Esc` clears the query, a second `Esc` closes.
- `fl_search::fts_terms` keeps negated terms when there is no positive one
  (`-word` alone now excludes; it used to be ignored).
- Tests: 9 UI tests in `views/queue/tests.rs` (focus, filter, selection,
  Esc, Enter, `/` from the pane, Save view, `set_filter`, reload keeps the
  filter), 9 in `palette/tests.rs` (open / toggle, word filter, enter runs,
  Esc twice and focus return, backdrop, actions follow focus, search hit
  opens, saved view opens with its filter, delete view); 6
  `queue_filter` tests in `fl-search` (incl. the broad per-row path);
  `crates/fl-db/tests/views.rs`. Timing tool:
  `cargo run -p fl-search --release --example queue_filter_bench -- fake-data/forkline.db`.
- Bench: step `search_p95_ms` (Issues queue, the 20 queries of the Tauri
  search bench, 11 rounds, first dropped; change -> first filtered page
  painted, mark `filter_painted`). `bench::list_built` keeps the Issues list
  view for it. `fl-bench gpui` now runs with no `--partial` gaps.

Bench (2026-11-17, release, AC power, 240 Hz, warm runs 2-4; run 1 cold
after the migration: `launch_ms` 647, `app_startup_ms` 136):
`search_p95_ms` 11.4-11.5 (30), `open_cached_item_ms` 6.3 (30),
`switch_tab_ms` 4.1-4.2 (8), `diff_scroll_fps` 239-240 (120), `launch_ms`
379-399 (600), `app_startup_ms` 40-58 (60). Full `fl-bench gpui` (no
`--partial`) passes.

Checked by eye (PrintWindow, dark, fake data): Inbox with the empty query
bar (`target/tmp/g1s6-inbox.png`); Issues filtered by `-is:pr fixture` with
`11443 results · 3 ms`, Save view, group count (`target/tmp/g1s6-f2.2.png`,
during a bench run). Not checked by eye: the palette (only keys open it;
covered by UI tests), the Views menu, light theme.

Open decisions (step 6):

- A saved view's name is its query text; no rename. A name prompt is a
  small add on Save view.
- With a filter, an Inbox notification without a cached issue (release,
  discussion) never matches: the filter reads issue columns.
- The repo filter menu applies the `/` filter synchronously on the UI
  thread (up to ~10 ms with a broad filter).
- While a new filter's page loads, the old rows stay and the result line
  says `Searching`; no spinner.
- The palette reads saved views and pinned repos once when it opens. Item
  actions show when the focused view handles them, not per item (e.g.
  Approve shows on an Inbox tab with no row selected; it then does nothing).
- Palette rows have no icons; the mockup's `>` prompt is a placeholder.
- A failed Save view / Delete view write is silent.
- The filter is not saved in the layout; a restored tab starts unfiltered.

Done (step 7, remaining views, 2026-11-18):

- `views/rows.rs`: the shared list for the flat views. `Rows<T>` holds the
  rows, the selection and the paging; a view implements `HasRows`
  (`rows`, `rows_mut`, `open_ref`). `Rows::load_now` reads the first page on
  the UI thread (first frame has rows), `reset` swaps the fetch (new filter
  or query: no selection, first page in the background, `first_page_ms`),
  `reload` re-reads the loaded pages and keeps the selection by key; a
  `generation` drops stale reads. `rows::list` is a `uniform_list` with a
  scrollbar that loads the next page near the end; `row_shell` gives the
  row look and clicks (click selects, double-click opens); `list_keys` adds
  the list actions.
- Keys: the list keys (`j` / `k` / arrows / `Enter`) also work in key
  context `List` (`(Queue || List) && !Input`); `/` in
  `(Queue || Reading || List) && !Input`; new `views::TogglePin` on `p` in
  `(List || Repo) && !Input`. `views::step` is the shared selection rule
  (none until the first move, then row 0).
- `views/repos.rs`: Repositories (`ViewRef::Repos`, hides Explore-only
  repos) and Starred (`ViewRef::Starred`), one view. Name filter input
  (`/` focuses, `Enter` returns and keeps it, `Esc` clears); rows: owner /
  name, star and pin marks, description, open PRs / issues, language, stars,
  Pin / Unpin button (`repo-pin`, ix). `Enter` opens the repo, `p` pins.
  `repos::set_pinned` writes, emits `repos`, and wakes sync on pin.
  Reloads on `repos` / `issues`. Ready keys `repos`, `starred`.
- `views/explore.rs` (new, no Tauri view): repos the people you follow
  starred, newest star first: "starred by a, b, c and N others · age",
  language, stars. `Enter` opens the repo. Data `fl_db::queries::explore::feed`
  (`fl_types::ExploreRow`), tests in `crates/fl-db/tests/explore.rs`; the
  per-row login query is in the query-plan test. Ready key `explore`.
- `views/repo.rs`: Repo overview (`ViewRef::Repo`): header with Pin
  (`repo-pin`), stats, open PR / issue cards (`repo-open-prs`,
  `repo-open-issues`; they open the repo's Pulls / Issues queue), languages
  bar, star history sparkline (GPUI `canvas` + `PathBuilder::stroke`), recent
  commits with check dots, releases, contributors. Dates `YYYY-MM-DD` UTC.
  `p` pins. Read in the background; reloads on `repos` / `issues`. Ready key
  `repo`.
- `views/search.rs`: Search (`ViewRef::Search { q }`): query bar,
  `fl_search::search` 50 per page, status line (`N+ results · X ms`, No
  results, Search failed), rows with state icon, `repo #n`, title and the
  snippet with hits in bold (U+E000 / U+E001 parsed by `split_snippet`),
  "In a comment:" for comment hits. Empty `q`: operator help, and the bar
  holds the focus when the tab opens. `Enter` in the bar runs the query in
  the same tab; `Esc` returns to the list; `/` focuses the bar. Ready key
  `search` (non-empty query only).
- `views/home.rs`: Home (`ViewRef::Home`): `lists::home_feed` (open items,
  newest update first), unread mark, comments, age. Reloads on `issues` /
  `pr`. Ready key `home`. Also holds the shared `state_icon` / `item_ref`
  for `IssueRow`.
- `workspace::SetViewRef(ViewRef)`: a view says it now shows another
  `ViewRef`; `ViewPanel` records it (the tab title follows) and the
  workspace saves the layout. Search uses it.
- `test_support`: `start_on` (open a view), `active::<T>`, `press`,
  `type_text`, `tabs`, `write` (SQL + `DataChanged`), `query_i64`.
- Tests: `repos/tests.rs` 11, `explore/tests.rs` 4, `repo/tests.rs` 8,
  `search/tests.rs` 7, `home/tests.rs` 5, `views::tests` 1; fl-db
  `tests/explore.rs` 3. `cargo test -p fl-app`: 106 pass.
- Bench: step `open_views` opens each view in a tab and records
  `open_<key>_ms` (open -> `<key>_painted`). No budgets yet; `fl-bench` now
  prints durations without a budget as `(no budget)`.

Bench (2026-11-18, release, AC power, fake data; run 1 cold: `launch_ms`
821, `app_startup_ms` 181; 5 warm runs): `open_home_ms` 4.5-6.3,
`open_repos_ms` 6.5-7.2, `open_starred_ms` 6.1-6.4, `open_explore_ms`
5.6-6.4, `open_repo_ms` 7.0-7.1, `open_search_ms` 35-40 (`fixture`, a broad
query: more than 2000 docs, recency order), `search_p95_ms` 11.5-14.8 (30),
`open_cached_item_ms` 6.0-6.1 (30), `switch_tab_ms` 4.1 (8),
`diff_scroll_fps` 240 (120), `launch_ms` 440-469 (600), `app_startup_ms`
47-56 in 3 runs, 60.2 and 64.5 in 2 (60). The misses sit between
`store_ready` and `workspace_new` (window creation), as in steps 3 and 5;
step 7 adds no code on the launch path.

Checked by eye (PrintWindow, dark, `--offline` on `target/tmp/shot-data`
with a saved layout holding the six tabs; script `target/tmp/shots.sh`,
images `target/tmp/g1s7-*.png`): Home, Repositories, Explore, Repository 1
(languages bar, sparkline, commits), Search `fixture` (snippets in bold).
Not checked by eye: Starred (same view as Repositories), selection after
`j` (covered by UI tests), filter typing, releases / contributors below the
fold, the empty-query help, light theme.

Parity: `docs/design/gpui-parity.md`. The Tauri app still has issue close /
reopen, labels and assignees, merge, Request changes / Comment reviews,
inline review comments, thread reply / resolve, GitHub notification done,
the sign-in dialog, failed-action Retry / Discard, `Ctrl+Tab` and the Inbox
unread toggle. The note recommends keeping `app/` and `ui/` until those
land. Not removed; the user decides.

Tab titles and Go to (step 7, after review, 2026-11-18):

- `ViewRef` stays id-only (saved layouts unchanged). `ViewPanel` reads the
  repo's `owner/name` (`fl_db::queries::repo::full_name`, one row by key,
  sync) when it is built and on `SetViewRef`, and again on a `repos`
  change. `ViewRef::title_with(name)`: Repo tab `owner/name`, filtered
  queues `owner/name: Pull requests` / `owner/name: Issues`; an unknown repo
  keeps "Repository N". Tests and `Workspace::tab_groups` use the panel
  title.
- The queue's repo filter menu (`Queue::pick_repo`) sends `SetViewRef`
  (Pulls / Issues only), so the title and the saved layout follow the filter.
- `workspace::GoTo(ViewRef)` / `Workspace::go_to`: select an open tab of the
  same kind (`ViewRef::same_kind`), keeping its query or repo filter; the
  focused pane first, then layout order; else open the given view. A Search
  tab also gets its bar focused with the query selected
  (`SearchView::select_bar`). The rail buttons and the palette's top "Go to"
  entries use it; the rail lights a button by kind. The palette's per-repo
  Go to entries and `Search for "…"` still match exactly (`OpenView`).
- Tests: `view_ref::tests::titles_name_the_repo`,
  `queue/tests.rs` `picking_a_repo_names_the_tab`, `repo/tests.rs`
  `the_tab_title_follows_a_rename`, `search/tests.rs`
  `the_rail_selects_the_open_search_tab_with_its_query_selected`,
  `workspace::tests::go_to_keeps_a_filtered_tab_and_prefers_the_focused_pane`;
  fl-db `tests/repo.rs` `full_name_joins_owner_and_name`.
  `cargo test -p fl-app`: 111 pass.

Explore sync plan (decided 2026-11-18; its own step, after sign-in and
Retry / Discard):

- On the 30-min repo tick (`REPOS_EVERY`), last in the tick; skip it when
  REST remaining is under `LOW_REMAINING` (50).
- `GET /user/following` (ETag via `list_cached`), at most 200 users, into
  `following`.
- Per user: first page of `GET /users/{login}/starred` (Accept
  `application/vnd.github.star+json` for `starred_at`, 30 per page, ETag).
  Keep 90 days of stars in `explore_stars`.
- New repos from it get `explore_only = 1`, `sync_enabled = 0` (migration
  0008 rules; sync clears the flag or deletes unreferenced Explore-only
  repos).
- Pinning an Explore-only repo sets `explore_only = 0`, `sync_enabled = 1`
  (`repo::set_pinned`). Unpin leaves it to the 0008 rules.
- Explore gets `p` and a Pin button per row.
- Tests: the fl-sync mock-server harness (`crates/fl-sync/tests/common`).

Open decisions (step 7):

- The Explore feed reads every `explore_stars` row per page (fine for recent
  stars of followed users; not index-ordered).
- `open_*_ms` have no budget. Suggest 30 ms for all but search; a broad
  search term takes 35-40 ms (the `fl_search` broad path).
- Search shows the loaded count (`50+ results`), not a total.
- Home lists open items newest first (Tauri parity); with Inbox as launch
  view it adds little.
- A failed pin write is silent.

Changed files (step 7): `crates/fl-app/src/views/{rows,repos,explore,repo,search,home}.rs`
and their `*/tests.rs`, `views/mod.rs`, `views/queue.rs` (test accessor),
`panel.rs`, `workspace.rs`, `rail.rs`, `palette.rs`, `view_ref.rs`, `bench.rs`,
`test_support.rs`; `crates/fl-db/src/queries/{explore,mod,repo}.rs`,
`crates/fl-db/tests/{explore,schema,repo}.rs`;
`crates/fl-types/src/repo.rs`; `crates/fl-bench/src/main.rs`;
`docs/design/gpui-parity.md`, `docs/design/ui-plan.md`.

Done (parity gaps 8 and 9, sign-in and failed actions, 2026-11-18, branch
`gpui-parity`):

- `signin.rs`: `SignIn` dialog, an overlay like the palette (priority 2,
  dimmed modal backdrop; a backdrop click does not close it). Element ids
  `signin-*`.
  - Device flow: "Sign in with GitHub" (only with a build-time client ID)
    calls `Engine::auth_start`, opens `verification_uri`, shows the code
    (Copy, "Open the page again") and polls `auth_poll` at the server's
    interval. Expired / denied / errors show under the body and return to
    the start.
  - PAT paste (new; the Tauri dialog had none): masked `Input` with
    Save token (Enter too) and a "Create one on GitHub" link
    (classic token, scopes `repo,read:org,notifications`). New
    `Engine::sign_in_with_token` (trims, checks `GET /user`, stores the token
    like the device flow through the shared `finish_sign_in`). A 401 shows
    "GitHub rejected this token."
  - `FORKLINE_GITHUB_TOKEN` set but rejected: only a note (the env token
    comes first, a pasted one would not be used).
  - `Esc` (key context `SignIn`, and `Escape` from the input), Not now /
    Cancel close it; focus goes back. The dialog emits `SignInEvent`; the
    workspace closes it.
- When it opens: the footer now reads `auth_state` with the status (on
  `sync` and every 10 s) and emits `FooterEvent::SignIn` when it first reads
  "signed out" and on every signed-in -> signed-out change. A GitHub 401 in
  sync or the action worker sets `token_rejected` and emits `sync`, so this
  is the `Unauthorized` reopen. Dismissed, it stays closed until that change
  again. Also: footer "Sign in to sync" link, palette "Sign in to GitHub"
  (`workspace::OpenSignIn`). Never in a bench run (`read_only`; no
  `auth_state`, no `gh` start).
- Footer (`footer.rs`): `N failed` opens the failed list (summary, error,
  Retry / Discard per row; `engine.retry` / `discard`); it closes when none
  is left. `@login` opens the account box: Sign out, or the env-override
  note. A failed Retry / Discard / Sign out shows in the footer.
- `Store::init` takes the `fl_sync::Config`. Tests use
  `Config::test(NO_GITHUB)` with an in-memory token, so they never read the
  real env / credential store / `gh`. `test_support::open_with(cx, dir, sql,
  api, token)` (mock GitHub, signed out with `None`) and `wait_for` (moved
  from the reading tests). `wiremock` is an fl-app dev-dependency.
- Tests: `signin/tests.rs` 7 (no dialog when signed in, opens signed out and
  stays dismissed, Esc and palette, pasted token signs in, rejected token
  error, rejected token reopens, device flow code then sign-in), footer
  `retry_and_discard_failed_actions`, fl-sync
  `a_pasted_token_signs_in_once_github_accepts_it`. `cargo test -p fl-app`:
  119 pass. Workspace clippy and tests pass.

Bench (2026-11-18, release, AC power, fake data; run 1 cold: `launch_ms`
607, `app_startup_ms` 66.5, 204 ms before `main`; warm runs 2-3):
`launch_ms` 442-444 (600), `app_startup_ms` 47-50 (60), `switch_tab_ms`
4.1-4.2 (8), `open_cached_item_ms` 5.9 (30), `search_p95_ms` 11.7-14.5 (30),
`diff_scroll_fps` 240 (120). No new code on the launch path but one
background `status` + `auth_state` read, skipped in bench.

Checked by eye (PrintWindow, dark, `--offline` on `target/tmp/shot-data`
with `gh` removed from PATH so the app reads "signed out"; script
`target/tmp/shot-signin.ps1`, image `target/tmp/g1p-signin.png`): dialog
with the no-client-ID note, token box focused, footer "Sign in to sync".
Signed in, the footer shows `@octocat`. Not checked by eye: the device code
screen (needs a client ID), the failed list and account box (need a click),
light theme.

Open decisions (gaps 8 / 9):

- A git 401 (`GitError::Unauthorized`, Files tab) does not reopen the
  dialog: the engine still holds the token as good. The Tauri app behaves
  the same (its dialog only shows while signed out).
- Sign out deletes the stored token only; with `gh` signed in the app stays
  signed in through `gh auth token` (Tauri: same).
- The pasted token's scopes are not checked; a token without
  `notifications` signs in and the inbox sync then fails with an error in
  the footer.
- While the dialog is open, workspace keys (`Ctrl+T`, `Ctrl+K`) still work
  under it.
- Signing in under `--offline` stores the token but starts no sync.

Changed files (gaps 8 / 9): `crates/fl-app/src/{signin.rs, signin/tests.rs,
footer.rs, workspace.rs, store.rs, main.rs, palette.rs, test_support.rs}`,
`views/reading/tests.rs`, `crates/fl-app/Cargo.toml`;
`crates/fl-sync/src/engine.rs`, `crates/fl-sync/tests/sync.rs`;
`docs/design/gpui-parity.md`.

Order of the remaining gaps agreed by the user on 2026-11-18 (see
`docs/design/gpui-parity.md`), and gap 7: `E` sends `NotificationDone`.

Done (parity step 1, small writes and keys, 2026-11-18, branch
`gpui-parity`):

- Close / Reopen (gap 1): action-bar button `action-close` /
  `action-reopen` and key `X` (`reading::ToggleState`, context
  `(Queue || Reading) && !Input`, forwarded by `ListView`; palette "Close or
  reopen"). `Action::SetState` through the queue; a merged PR has neither.
  A second `X` before the first is sent supersedes it (`pending_fields`).
- Merge (gap 3): `action-merge` menu (Merge commit / Squash and merge /
  Rebase and merge), then `action-merge-confirm` / `action-merge-cancel`
  in the bar; no key. Disabled with the reason for a draft, a conflict
  (`mergeable = CONFLICTING`) or a pending change on the PR (the Tauri
  checks). Confirm re-checks; another item drops the pick
  (`ReadingPane::merge_confirm`). `Action::Merge` sets `state = merged` at
  once.
- `E` (gap 7) keeps the local `done_at` and also queues
  `Action::NotificationDone` (GitHub "done", `DELETE
  /notifications/threads/{id}`). Sync clears `done` on new activity, as
  `done_at` does.
- `Ctrl+Tab` / `Ctrl+Shift+Tab` (gap 10): `workspace::NextTab` /
  `PrevTab` select and focus the next / previous tab of the focused pane
  (wraps). Palette "Next tab" / "Previous tab".
- Unread (gap 11): `is:unread` in `fl_search` (`Filter::Unread`). Queues
  other than the Inbox and Search read `updated_at > last_viewed_at`; the
  Inbox reads `notifications.unread` (its row mark), also for a
  notification without a cached issue. `queue_filter` now takes the
  `QueueKind`. Every queue's `/` bar has an `Unread` toggle
  (`queue-unread`) that adds or removes the word `is:unread`; saved views
  can hold it. Search help lists it.
- Not done from gap 11: the Tauri grouped / flat toggle. GPUI queues are
  always grouped; ask if flat is wanted.
- Tests: reading `x_closes_then_reopens_the_item`,
  `merge_waits_for_the_confirm`, `merge_cancel_and_other_items_drop_the_pick`,
  `merge_is_blocked_for_a_draft_or_a_conflict`, the `E` test now checks
  `notification_done`; queue `the_unread_button_keeps_unread_notifications`,
  `toggle_term_adds_and_removes_one_word`; workspace
  `ctrl_tab_cycles_the_focused_pane`; fl-search
  `unread_reads_the_issue_or_the_notification`. `cargo test -p fl-app`: 126
  pass. Workspace clippy and tests pass.

Bench (2026-11-18, release, fake data, ON BATTERY: the laptop came off AC
during the run; 3 runs, run 1 cold): `launch_ms` 374-400 warm (600),
`app_startup_ms` 47-52 warm (60), `search_p95_ms` 18.6 (30),
`open_cached_item_ms` 17.1 (30), `switch_tab_ms` 9.7-9.8 (8, MISSED),
`diff_scroll_fps` 102 (120, MISSED). The same whole-frame pattern as the
step-4 battery runs (9.7 / 16.5, back to 4.0 / 6.2 on AC). Re-run on AC
power before shipping.

Checked by eye (PrintWindow, dark, `--offline` on `target/tmp/shot-data`,
image `target/tmp/g1p-actions.png`): PR #3 action bar with Approve A,
Reply R, Close X, Merge menu button. Not checked by eye: the open menu,
the confirm row, a blocked Merge, Reopen, the Unread toggle (needs a click
or the Inbox tab on show), light theme.

Step-1 decisions (settled 2026-11-18, see "Decided (2026-11-18, steps 1-5)"):

- `X` closes without a confirm (Reopen undoes it). Merge has a confirm and
  no key.
- Merge does not wait for checks or reviews (GitHub refuses; the action
  rolls back and shows in the failed list).
- The default merge method is not remembered; the menu asks each time.

Changed files (step 1): `crates/fl-search/src/{lib,query,tests}.rs`,
`crates/fl-search/examples/queue_filter_bench.rs`;
`crates/fl-app/src/views/{queue.rs, queue/tests.rs, reading.rs,
reading/action_bar.rs, reading/tests.rs, list_view.rs, mod.rs, search.rs}`,
`crates/fl-app/src/{workspace,palette}.rs`.

Done (parity step 2, review composer, 2026-11-18, branch `gpui-parity`):

- Gap 4: on an open PR the `R` box is the review composer. Buttons
  `reply-send` (Comment, `Ctrl+Enter`), `review-approve`,
  `review-request-changes` (disabled while the box is empty; GitHub rejects
  an empty request). Approve and Request changes send
  `Action::SubmitReview` with the text as the body; the local review row
  shows at once. An issue or a closed PR keeps the plain reply box. `A`
  still approves with no body.
- Comment without inline drafts stays a plain PR comment
  (`Action::Comment`), not a `COMMENT` review. Step 5 sends a `COMMENT`
  review when drafts exist.
- Tests: reading `request_changes_sends_the_body_as_a_review`,
  `approve_from_the_composer_keeps_the_body`,
  `request_changes_needs_a_body`, `an_issue_reply_has_no_review_buttons`.

Done (parity step 3, labels and assignees, 2026-11-18):

- Gap 2: `reading/pickers.rs`. `L` or the bar button `action-labels` opens
  the label picker; `action-assignees` the assignee picker (no key: `A` is
  Approve). Palette "Edit labels" / "Edit assignees". The picker sits under
  the action bar (inline, not floating): a search box (case-insensitive
  substring; Enter toggles the first match) and rows `("pick", id)` with a
  check mark and the label colour. Esc (context `Picker`) closes it and
  focus returns. Another item drops it.
- Each click sends the whole set (`SetLabels` / `SetAssignees`), as the
  Tauri view does; a newer set supersedes one not sent yet. The picker keeps
  its own set, so fast clicks do not read a stale reload. Works on issues
  and PRs.
- Header: `Assigned: a, b`, and `labels pending` / `assignees pending`
  pills (`labels-pending`, `assignees-pending`) from `pending_fields`.
- `fl_db::queries::issue::assignees` (new, used by `thread` and the pane).
  The pane reads assignees for PRs too (`PrDetail` has none; the DTO is
  unchanged, so the Tauri bindings stay as they are).
- Choices: `issue::edit_options` (Tauri parity): the repo's labels, and the
  first 100 known users by login. A user past 100 cannot be picked yet.
- Tests: reading `l_opens_the_labels_of_the_repo_and_a_click_sends_the_set`,
  `the_assignee_search_and_enter_pick_the_first_match`,
  `escape_closes_the_picker_and_another_item_drops_it`.

Done (parity step 4, threads, 2026-11-18):

- Gap 6: Files rail cards (`reading/files.rs`) have `("thread-reply", ix)`
  and `("thread-resolve", ix)` (Resolve / Unresolve). Reply opens one box
  under the card (`Ctrl+Enter` or `thread-reply-send`; Esc or
  `thread-reply-cancel`; focus returns). `Action::ReplyThread` /
  `Action::ResolveThread` through the queue; a second Resolve click
  supersedes one not sent yet.
- The tab now subscribes to `DataChanged`: a `pr` change re-reads the
  threads (not the patch). Cards keep their order; new threads go last.
- Cards show every comment of an open thread (before: 3) and the first
  comment of a resolved one, as GitHub does; a local reply shows `sending`,
  a queued resolve `syncing`. Errors show at the top of the rail
  (`files-notice`). Only the header part of a card scrolls the diff.
- Tauri parity: no reply on a resolved thread.
- Tests: files `resolve_then_unresolve_a_thread`,
  `reply_in_a_thread_with_ctrl_enter`, `escape_cancels_a_thread_reply`.

Done (parity step 5, inline review comments, 2026-11-18):

- Gap 5: the pinned gpui-component `Diff` has what it needs:
  `on_add_annotation` (an add button beside the hovered line and at the end
  of a line-number selection), `annotations` + `render_annotation` (content
  under a line). No fork or workaround.
- Select lines by a click or drag on the line numbers; then the add button
  at the end of the selection, or the toolbar `files-comment` (Comment),
  opens the box. The add button is not shown on hover without a selection:
  with `on_add_annotation` set, the `Diff` builds a hidden button (with a
  formatted label) on every visible line each frame, and
  `diff_scroll_fps` fell from 240 to 72-77. It is set only while
  `selected_lines()` is some (a `DiffEvent` subscription re-renders).
- `reading/files.rs`: the add button opens a box under the line
  (`draft-composer`; `Ctrl+Enter` or `draft-add` "Add to review"; Esc or
  `draft-cancel`). A draft is a dashed card under its line with
  `("draft-remove", ix)`. Drafts live in the Files tab (`FilesTab::drafts`)
  and are dropped when another item shows (as in Tauri, where they are
  component state). The Files tab label reads `Files 3 · 2 pending`.
- Range rules (`draft_for`): a range on one side is a multi-line comment
  (`start_line` < `line`, any drag order); a Unified range across removed
  and added lines is a comment on its last line.
- Sending: the `R` composer says `N inline comments go with it`
  (`review-drafts`). With drafts, Comment (`Ctrl+Enter`, empty text allowed)
  sends a `COMMENT` review; Approve, Request changes and `A` carry them too.
  The drafts leave the tab when queued; a rejected review puts them back.
  Local apply turns them into threads; the rail reloads on `pr`.
- Known gap stays: a range across two hunks is sent as is; GitHub rejects
  it and the review rolls back.
- Tests: files `a_range_on_one_side_is_a_multi_line_draft`,
  `drafts_go_with_the_next_review`, `approve_takes_the_drafts_too`,
  `escape_cancels_the_box_and_remove_drops_a_draft`.
  `cargo test -p fl-app`: 140 pass. Workspace clippy and tests pass.

Done (Explore sync, 2026-11-18, the plan above):

- `fl_sync::sync::explore`, last on the 30-min repo tick (engine
  `try_sync`), skipped when REST remaining < `LOW_REMAINING`; the per-user
  loop also stops there (no error; the next tick goes on).
  `GET /user/following` (ETag, `list_cached`, at most `MAX_FOLLOWING` = 200)
  replaces `following` (unfollow cascades the stars). Per followed user:
  first page of `/users/{login}/starred?per_page=30`, Accept
  `application/vnd.github.star+json`, ETag in `sync_state`
  `explore:{user_id}`; a 200 replaces that user's `explore_stars`, stars
  older than `EXPLORE_DAYS` (90) are skipped and pruned.
- A repo first seen there gets `explore_only = 1`, `sync_enabled = 0`.
  `sync::repos` clears `explore_only` for repos in the user's own or
  starred lists. Explore-only, unpinned repos no star, notification or
  issue references are deleted at the end of the run.
- `Github::rest`: an `Accept` in `extra` replaces the default media type.
- `repo::set_pinned`: pinning sets `explore_only = 0` and turns sync on for
  an Explore-only repo (a normal repo keeps its `sync_enabled`). Unpin
  leaves the flags.
- Explore view: `p` and `("explore-pin", ix)` Pin / Unpin per row, pin
  icon. `ExploreRow.pinned` (new field; no Tauri binding uses the type).
- Explore errors other than rate limit / auth / network / db show in the
  footer status (`Explore: ...`) and do not stop the rest of the tick.
- Tests: fl-sync `explore_reads_following_and_their_recent_stars`,
  `explore_sends_the_star_etag_and_a_listed_repo_clears_explore_only`
  (common env mounts an empty `/user/following` at the lowest priority);
  fl-db `pinning_an_explore_only_repo_syncs_it`; fl-app explore
  `p_pins_an_explore_repo_and_so_syncs_it`, `the_pin_button_pins_its_row`.

Bench after Explore sync and the add-button fix (AC power, 5 runs, run 1
cold): `diff_scroll_fps` 240, `switch_tab_ms` 4.0-4.1, `search_p95_ms`
14.6-14.9, `app_startup_ms` 47-48, warm `launch_ms` 498-556 (600). The
launch rise over the run below is platform start (`main` to
`run_callback` 410-470 ms), before Forkline code runs.

Bench (2026-11-18, after parity steps 2-5, release, fake data, ON AC
POWER, `PowerOnline` True; 3 runs, run 1 cold): warm `launch_ms` 400-421
(600), `app_startup_ms` 40-42 (60), `switch_tab_ms` 4.1-4.2 (8),
`diff_scroll_fps` 240 (120), `search_p95_ms` 11.4-11.5 (30),
`open_cached_item_ms` 5.8-5.9 (30). Run 1 (first start after the build)
missed launch at 713 ms: `main` came 355 ms after spawn (exe load), as in
earlier cold runs. The step-1 battery misses are gone on AC.

Not checked by eye (parity steps 2-5): the composer buttons, the pickers,
the thread buttons, draft cards. Opening them needs a click or a key in the
running app; covered by UI tests only.

Decided (2026-11-18, steps 1-5, with the user):

- `X` closes with no confirm (Reopen undoes it).
- Merge sends at once; no wait for checks or reviews (GitHub branch
  protection refuses; the failed action shows the error). Show the check
  state beside the Merge button (add it if it is not there).
- Remember the merge method per repo: local state, migration 0011. Before
  the first merge in a repo the method is merge commit. Save the method
  when the merge is sent (not on GitHub confirm).
- Comment without inline drafts stays a plain PR comment.
- Assignee picker: first 100 known users, no server search.
- Pickers stay inline under the action bar.
- No key for assignees.
- The add button shows only while lines are selected (scroll fps).
- Drafts: keep them in memory per PR for the session (no DB). A "drafts"
  mark shows on the PR's list row. A warning shows on app quit and on
  reading-tab close when drafts are not sent; no dialog on item switch.
  Drafts do not move when the PR gets new commits; if GitHub rejects them,
  show the error and keep them (the existing restore path).
- Gap 11 rest, grouped / flat toggle: in every queue that groups now (like
  Unread). Default grouped. "Grouped" button, `g` key (do not make `g` a
  prefix key later) and a palette entry. Saved per queue tab in the layout
  state; `LAYOUT_VERSION` stays 2. Flat rows show the repo name. (Tauri:
  Inbox only, default flat, not saved.)

Done (G1 parts 1-3, 2026-11-18, branch `gpui-parity`, not committed):

1. Grouped / flat (gap 11 rest). Every queue has a `queue-grouped` button
   ("Grouped", on when grouped), `g` (`queue::ToggleGrouped`, context
   `(Queue || Reading) && !Input`, forwarded by `ListView`) and palette
   "Grouped or flat". Default grouped. `QueueQuery.flat` (new,
   `#[serde(default)]`): the Inbox reads `ORDER BY updated_at DESC` with no
   group first; Pulls / Issues / Reviews keep their order and lose their one
   header. Toggling reads again like a repo change (selection drops) and
   keeps the `/` filter. Saved per tab: `ViewPanel::dump` adds `"flat": true`
   beside the `ViewRef` fields (`ViewRef` unchanged, it ignores the key);
   `LAYOUT_VERSION` stays 2. New `workspace::SaveLayout` action. Rows already
   show `owner/name #n` in both modes.
2. Drafts per PR for the session: `crates/fl-app/src/drafts.rs` (global
   `Drafts` entity, `HashMap<pr id, Vec<DraftReviewComment>>`, memory only).
   The Files tab, the Files label count, the `R` composer and `A` read it, so
   drafts outlive an item switch (no dialog) and a closed tab. Queue rows of
   a PR with drafts show a dashed `drafts` mark (`("queue-drafts", ix)`). A
   PR tab with drafts reports `closable = false` (the dock draws no X and
   ignores `Ctrl+W`), draws its own `tab-close-drafts` button, and `Ctrl+W`
   or that button asks "Close this tab?" (platform prompt); "Close tab" sends
   `workspace::RemoveTab`. Window close with drafts asks "Quit Forkline?"
   (`on_window_should_close`). New commits leave drafts as they are. A
   review the queue rejects puts its drafts back (`Drafts::restore`); a
   review GitHub rejects later rolls back and stays in the failed list
   (Retry resends the same comments), as before.
3. Merge method per repo: migration 0011 `repo_merge_methods` (repo_id PK,
   cascade on repo delete); `repo::merge_method` / `set_merge_method`;
   `MergeMethod::as_str` / `parse`. The bar shows `action-merge` labelled with
   the repo's method (merge commit before the first merge) and
   `action-merge-method` (caret menu, current method checked); both go to the
   confirm row. Confirm saves the method at once, then queues the merge.
   `merge-checks` beside Merge (also when blocked): Checks pass / fail /
   running from `pr_details.check_state`; nothing when it is NULL.

Tests: queue `g_and_the_grouped_button_switch_to_one_list`,
`g_works_from_the_reading_pane_and_keeps_the_filter`; workspace
`a_flat_queue_tab_restores_flat`; fl-db `flat_inbox_is_newest_first`,
`merge_method_is_kept_per_repo` (schema plan test covers flat queries);
files `drafts_stay_with_the_pr_across_an_item_switch`,
`closing_a_pr_tab_with_drafts_asks_first`,
`a_pr_tab_without_drafts_closes_at_once`, `quit_with_drafts_asks_first`;
reading `the_merge_button_offers_the_method_last_sent_in_the_repo`,
`the_check_state_shows_beside_merge`. `cargo test -p fl-app`: 152 pass.
Workspace clippy (`-D warnings`) and tests pass.

Bench (2026-11-18, release, fake data, AC power, `PowerOnline` True). The
machine was noisy (single runs of both builds missed launch, diff fps and
search at random), so HEAD `88dcf10` was built in a temp worktree and run
interleaved with this build, 7 pairs. Warm runs, base vs new: `launch_ms`
447-539 vs 476-518 (600), `app_startup_ms` 50.6-63.9 vs 51.2-58.2 (60),
`switch_tab_ms` 3.9-4.2 vs 3.9-4.2, `search_p95_ms` 14.6-14.9 vs 14.6-14.8
(one 62 ms outlier each), `open_cached_item_ms` 6.1-9.1 vs 6.0-9.5,
`diff_scroll_fps` 240 in most runs of both (81 / 74 / 155 / 177 outliers in
both). No difference between the builds within noise.

Checked by eye (PrintWindow, dark, `--offline` on `target/tmp/shot-g1`,
image `target/tmp/g1-grouped.png`): Inbox with the Grouped button on and the
group headers. The `/` placeholder is cut shorter now ("Filter: is:open
aut").

Manual pass (2026-11-18, by the user, release, `--offline` on a copy of
fake data): flat mode (`g`, button, palette), flat saved per tab across a
restart, the drafts mark, drafts kept across an item switch, the PR tab
close button and `Ctrl+W` prompt, the quit prompt, the split Merge button
with the repo's method, the check state. All work.

Open decisions (G1 parts 1-3):

- Only `ViewRef::Pr` tabs warn on close. A queue tab whose reading pane
  shows a PR with drafts closes at once (the drafts stay in memory).
- While a PR tab has drafts the dock's own X is replaced by the tab's
  button, drawn in the tab title.

Tauri removal (2026-11-18, with the user): deleted `app/`, `ui/`, the Bun /
Vite / ESLint / TypeScript config, `bench/budgets.json`, the `specta`
feature of `fl-types` and the Tauri / specta workspace dependencies.
`fl-bench` checks only the GPUI app (`gpui` is optional). The `fl-app`
package keeps its name; its binary is `forkline` (ADR 0001), so the app is
`target/release/forkline.exe`. The device flow stays: the GPUI sign-in
dialog uses it. The `ui_layout` settings row of old databases is left
unused. Cargo.lock lost 167 packages; no kept package changed version.

Bench after the removal (release, fake data, AC power): 3 single runs missed
at random (`search_p95_ms` 62-64, `diff_scroll_fps` 30, one `launch_ms` 659,
one `app_startup_ms` 60.6). An interleaved A/B with the pre-removal
`fl-app.exe` (3 pairs) gave the same numbers in each pair: `launch_ms`
427-486 vs 435-492, `app_startup_ms` 45-47 vs 46-50, `search_p95_ms`
12-24 vs 12-32, `diff_scroll_fps` 240 / 86 / 79 vs 204 / 85 / 82,
`switch_tab_ms` about 4 in both. Machine noise, not the removal.

Smooth wheel scroll (2026-11-18, found in the manual pass: the Conversation
tab "moves in jumps" with a mouse wheel):

- Cause: frames are fast (a frame log of a real scroll: 3.4 ms p50, 4.5 ms
  p99 from render to painted; an idle pane renders 0 times a second). GPUI
  on Windows turns a wheel notch into 3 lines, and `list` moves all 60 px
  in one frame. GPUI and gpui-component have no smooth wheel scroll.
- Fix: `crates/fl-app/src/smooth_scroll.rs` (`SmoothWheel`). A canvas
  under the list takes line-based wheel events in the capture phase and
  plays the distance over about 100 ms (ease-out, time constant 35 ms). A
  turn the other way drops the rest. Pixel deltas (touchpads) and
  horizontal scrolls go to the list as before. Used by the Conversation
  tab only.
- Test: `a_wheel_notch_scrolls_the_conversation_smoothly` (red without the
  fix: "the notch landed in one frame: 60").
- Bench: new steps `conversation_scroll_fps` (PR tab of the fake PR with
  the most comments) and `inbox_conversation_scroll_fps` (Inbox reading
  pane), 40 px a frame down and back up for 2 s; no budget. 132-240 fps.
  They call `ListState::scroll_by`, so they measure drawing, not the wheel.
- Not done (the user: "not for now"): the same smooth scroll for the queue
  lists and the Files diff, which still jump one notch at a time. About 5
  lines per list.

Next: register the GitHub OAuth App, then the manual pass against a real
account (see "Next" near the top).

Decided (2026-11-16):

- Keep the Other inbox group. Done (step 4) is how the user clears it.
- The queue selects no row at launch or after a filter change, so opening the
  app does not mark the top item read. The first `j` selects the first row.

Open decisions:

- Fake timestamps are in the future, so every age reads `now`.
- Markdown body text renders at 16 px; `.text_sm()` on `TextView` did not
  change it. Polish pass: find how the pinned TextView takes its body size.
- Tab titles are still `Pull request 3` / `Issue 14119`; the pane knows the
  number and title. Show `#3 title` (needs a way for a view to set its tab title).
- A restored layout builds every tab, so a background Issue / PR tab marks
  its item read at launch. It was opened before; accept, or defer the read to
  first show.
- `fl-fake`: `pr_details.changed_files` disagrees with the demo git diff
  (PR #3: 2 vs 1). The Files tab label switches to git's count once loaded.
- A failed `enqueue` of `NotificationRead` is dropped silently (detached task).
- Snooze is a fixed day. A menu (later today / tomorrow / next week) is a
  small add on the Snooze button.
- An expired snooze shows again only at the next queue reload (any `inbox`,
  `issues` or `pr` change); no timer.
- Done / Snooze work on Inbox rows only. Pulls / Issues / Reviews rows are
  issues, not notifications; triage there needs its own columns.
- No undo for Done / Snooze yet.
- Approve shows for any open PR, also the user's own (GitHub rejects that;
  the action rolls back and the error shows in the bar).
- Files tab (step 5):
  - No syntax colours in the diff. `Diff::syntax_highlight` is on, but the
    `DiffState` needs a highlighter (`with_syntax_highlighter`); wire
    gpui-component's highlighter in the polish pass.
  - The whole patch is read and parsed at once. A very large PR (tens of
    MB) has no cap or per-file lazy load yet.
  - Beside a queue the pane is narrow: list 220 + rail 240 leave little for
    the diff. Options: hide the rail under a width, or a key to toggle it.
  - The current file follows clicks and keys, not the scroll position
    (`DiffState` exposes no scroll offset).
  - No inline thread display in the diff (only a tint); threads are read
    only (no reply / resolve from the rail yet). No comment-on-line.
  - The mockup's bottom bar (Approve / Request changes / Comment) is not
    repeated; the action bar above serves it. No Request changes action.
  - The Files tab label uses `pr_details.changed_files` until the patch is
    read, then the patch's count.

## After G1: look refresh and colour themes (planned 2026-11-17, nothing built yet)

Plan: `docs/design/themes-plan.md` (GPUI G2), after G1 steps 6 and 7. Own
palette and type scale in place of the `gpui-component` defaults, eight
built-in themes, a custom accent the user picks (rest of the palette derived
from it), light theme parity, a choice of eleven interface and ten code fonts
(defaults Inter and Obel Mono), a colour per repository, and a "Hide secrets in code" switch (Obel Mono's
masking).

G2 step 0, G1 check (2026-11-18, on `main` at `1d8c6e5`): G1 steps 1-7 are
done in code and docs; `ui-plan.md`'s status line still says the Tauri removal
waits (stale). Clippy passes, 241 tests pass. `fl-bench gpui`, 3 runs while
Chrome and Surfshark were using CPU: `launch_ms` 621-931 (600) and
`app_startup_ms` 75-179 (60) missed every run; `search_p95_ms` and
`diff_scroll_fps` missed only the first, cold run (14.8 / 229 after that);
`switch_tab_ms` 4.1-4.3, `open_cached_item_ms` 9.0-9.6. The user accepted the
startup misses as machine noise. Re-take the baseline on a quiet machine
before step 1 so G2's final bench has a number to compare with. `ui/dist` and
`node_modules/` are untracked leftovers of the Tauri app; safe to delete.

G2 baseline (2026-11-18, `main` at `1d8c6e5`, 5 runs, machine busy: CPU
17 %, Chrome and Surfshark): `launch_ms` 464-545, `app_startup_ms` 50.7-60.6
(2 of 5 missed), `switch_tab_ms` 4.0-4.2, `diff_scroll_fps` 240 in 3 runs
(86 and 30 in the others), `search_p95_ms` 14.6-65.9 (4 of 5 missed),
`open_cached_item_ms` 5.7-8.8, `store_ready` -> `workspace_new` 24-37 ms.

## GPUI G2 state

Branch `gpui-g2`. Plan: `docs/design/themes-plan.md`. Mockups (the canvas
boards without their fonts): `docs/design/mockups/`. The warm light theme is
**Paper** (the board's name), not Linen.

Done (step 1, look refresh, 2026-11-19):

- `crates/fl-app/src/theme.rs` owns every colour and the type scale.
  `Palette` is a GPUI global (`cx.palette()` through `ActivePalette`);
  `theme::apply(mode, window, cx)` calls `Theme::change` /
  `sync_system_appearance`, then writes the palette over the kit theme
  (`write_kit`: colours, `font_size` 13.5, radius, highlight theme), so kit
  widgets match. Midnight and Daylight only. Tokens: surfaces (`bg`,
  `surface`, `raised`, `control`, `hover`, `selected`, `border`,
  `border_strong`, `backdrop`), text (`text_strong`, `text`, `text_body`,
  `text_read`, `text_muted`, `text_faint`), accent (`accent`, `accent_text`,
  `accent_fill`, `accent_fill_hover`, `on_accent`, `star`), GitHub states as
  `StateColor { fg, fill, on_fill }` (`open`, `merged`, `closed`, `pending`,
  `neutral`), `added` / `removed`, diff and `syntax`, plus `avatar(login)` and
  `label(color)`. Views read no `cx.theme()` colours any more (only
  `mono_font_family`).
- Deviation from the board: white text on the board's accent
  `hsl(228,92%,69%)` is 3.3:1, below the plan's 4.5:1, so primary buttons
  use `accent_fill` `hsl(228,72%,57%)`. Accent text, dots and the tab
  underline keep the board's value.
- Title bar: **native**, not the board's custom bar. A custom bar needs the
  kit's `TitleBar::window_options` (a transparent, client-drawn title bar),
  and on Windows that slowed every present: `diff_scroll_fps` 240 -> 102.6
  (the same in every run), `switch_tab_ms` 4.1 -> 9.6, `open_cached_item_ms`
  6 -> 15. Default window options brought all three back. `main.rs`
  `window_options` holds this; the test
  `the_main_window_keeps_the_native_title_bar` fails if a transparent title
  bar comes back. (Ruled out first: the tree-sitter grammars; with them
  removed the numbers stayed the same.)
- Rail: 52 px, 34 px buttons, unread count badge, a divider after Reviews,
  Search kept (no title bar field). Settings left the rail (Ctrl+K reaches
  it; step 5's "Appearance…" replaces it). The signed-in avatar at the
  bottom opens the account box (moved from the footer).
- Dock tabs: view icon, item tabs read `#42 title` with a state-coloured
  icon (`issue::tab_label`, one synchronous row read, refreshed on `issues`
  / `pr`).
- Queue: 48 px header (17 px title, count, outlined repo filter), 30 px
  sentence-case group headings without fill, 54 px rounded rows without
  borders, repo in the code font, author avatar, unread dot, tinted
  selection. `rows.rs` rows (Home, Repositories, Starred, Explore, Search)
  are rounded and inset the same way.
- Reading pane: 21 px title, state pill with icon, `author wants to merge
  into base`, `+N -N`, `n of m checks`, label pills outlined in the label's
  hue (`Palette::label`), Approve as the one primary button, tab counts as
  badges (Checks tinted while failing / pending), conversation as avatars and
  bordered cards. Markdown body text is 13.5 px now (the kit's `font_size`).
- Footer: sync icon, key hints on the right.
- Syntax colours in diffs and code blocks: `gpui-kit` tree-sitter features
  for 17 languages (`crates/fl-app/Cargo.toml`); colours from
  `Palette::syntax` through the kit's highlight theme. The kit's diff tints
  lines with `success` / `danger` at 12 %, so the diff follows the theme.

Tests: `theme/tests.rs` (contrast rules for both palettes: text, muted text,
accent, button text, states, pills, diff, syntax, avatars, label hues every
5 degrees; stable avatar hash; saved mode parsing), `rail.rs` initials, queue
`switching_theme_keeps_the_selection`, `main.rs`
`the_main_window_keeps_the_native_title_bar`. Tab-title expectations now
read `#N title`.

Bench after step 1 (release, 3 runs, Chrome and Surfshark still running):
`launch_ms` 781 / 408 / 401 (the first run was a cold load of the new exe:
422 ms before `main`), `app_startup_ms` 42-45, `switch_tab_ms` 4.1-4.2,
`open_cached_item_ms` 5.6-5.9, `diff_scroll_fps` 240 / 30 / 240,
`search_p95_ms` 11.3 / 66.7 / 11.3 (the 30 fps and 66 ms misses are the
same noise the baseline had), `store_ready` -> `workspace_new` 23-26 ms.
Clippy passes; `cargo test --workspace` 251 pass.

Checked by eye (PrintWindow, `--offline` on copies of `target/tmp/shot-data`
with `theme` set; Files tab during a release `--bench` run on `fake-data`,
dark and light): rail, footer, Inbox queue, PR conversation, Files tab with
syntax colours (the Files shots still show the removed custom bar). Images: `target/tmp/g2s1-*.png`.

Open items from step 1:

- Exe size: 23.5 MB -> 49 MB, almost all from the tree-sitter grammars.
  Decided (2026-11-19): keep all 17 languages; the size is accepted, also
  with the 8-12 MB the fonts step adds.
- The dock's active tab is square: the kit's `Tab` variant has no radius and
  the dock builds its own tabs. Rounded needs a custom `DockAreaRenderer`.
- Action buttons are the kit's `small` size (about 24 px); the board has
  30 px.
- The diff's add / remove backgrounds come from the kit (`success` /
  `danger` at 12 %); `diff_add_bg` / `diff_del_bg` are not used by it.
- System mode does not follow a live OS theme change (as before).

Done (step 2, all built-in themes, 2026-11-19):

- `theme.rs` has all eight palettes: Midnight, Graphite, Fjord, Forest,
  Rosé (`rose`), High contrast (`high-contrast`) dark; Daylight, Paper light.
  `THEMES` lists them. Board values (`theme-options.html`) for bg, surface,
  raised, selected, border, text, muted, accent, on-accent and star; the
  other tokens are worked out per theme. Each theme has its own GitHub state
  colours, diff colours and syntax colours (hue fixed, lightness tuned for
  its backgrounds). Accent buttons on the dark themes other than Midnight use
  the board's dark on-accent text, so `accent_fill` = the board's accent.
  Paper's muted text is `hsl(30,8%,35%)`, not the board's 38 %: 38 % is
  below 4.5:1 on `raised`.
- Picking a theme: settings `colors_dark` / `colors_light`, value
  `{"kind":"theme","name":"graphite"}` (the plan's model). `main.rs` reads
  them with `theme` in one DB read and sets the `theme::Choices` global;
  `theme::apply` uses it for the current mode. Missing, malformed or
  unknown values, `{"kind":"accent"}` (until step 4) and a theme of the
  other mode load as Midnight / Daylight. No UI yet (step 5); set the rows
  with `sqlite3` to try one.
- Repo view: commits without a status had a `border`-coloured dot, almost
  invisible on the light themes; now `text_faint`.

Tests: the contrast tests in `theme/tests.rs` run over all eight themes;
new: unique names and exactly Daylight / Paper light, `Choices` parsing and
fallbacks, `apply` uses the chosen theme in each mode (kit background too).
Clippy passes; `cargo test --workspace` 254 pass.

Checked by eye (PrintWindow, release, `--offline` on copies of
`target/tmp/shot-data` with `theme` + `colors_*` set and a saved layout per
view; scripts `target/tmp/g2/shots.sh` and `files.sh`; images
`target/tmp/g2s2-*.png`): Paper: Home, Inbox, Pull requests, Issues,
Reviews, Starred, Repositories, Explore, repo overview, issue, PR
conversation, Search, Settings, and the Files tab (10k-line diff, during a
`--bench` run on a copy of `fake-data`). Inbox and PR conversation in
Graphite, Fjord, Forest, Rosé and High contrast; Daylight PR and repo
overview; High contrast Files tab.

Open items from step 2 (not theme work; seen in the shots):

- Fixed (`335b2f0`): Home / Repositories / Starred / Explore / Search rows
  are full width again (step 1 had dropped `w_full` when it added the
  inset; the inset is now the list's padding), so their right-hand columns
  line up.
- Fixed (`335b2f0`): the reading header's check icon follows GitHub's
  roll-up `check_state`, as the merge row does (the check list decides
  only without one); `fl-fake` derives the roll-up from its checks.
  Regenerate fake data to see consistent states.
- Fixed (decided 2026-11-19): the native title bar followed only the
  Windows app mode (GPUI sets `DWMWA_USE_IMMERSIVE_DARK_MODE` from the OS
  appearance), so it stayed dark in the light themes. New crate
  `crates/fl-win`: the **only crate allowed to use `unsafe`** (its own
  `[lints]`; `undocumented_unsafe_blocks = "deny"`), with safe wrappers over
  Win32 calls GPUI does not expose. `fl_win::set_dark_title_bar(hwnd, dark)`
  is called by `theme::sync_title_bar` from `theme::apply` (with a window)
  and in `main.rs` while the window is built (before the first frame). Test
  windows have no Win32 handle, so it is a no-op in tests. Checked by eye:
  Paper light bar, Midnight dark bar (`target/tmp/g2s3tb-*.png`). Open: if
  the OS switches app mode while Forkline runs, GPUI sets the bar from the
  OS again until the next theme change.

Done (step 3, repository colours, 2026-11-19):

- Migration 0012: `repos.color` (0-9, NULL = from the name's hash; sync's
  upsert does not list it, so it survives a re-sync).
  `fl_db::queries::repo::{colors, set_color}`; `issue::tab_label` also
  returns `owner/name`.
- `theme.rs`: `REPO_HUES` (10 hues with names), `Palette::repo(index)`
  (per-theme saturation / lightness in `Palette::repo`; a hue within 15
  degrees of the accent moves 15 degrees away), `RepoColors` global
  (picked colours, loaded in `main.rs` at startup; index = pick, else FNV
  hash of `owner/name` % 10), `theme::repo_color(full_name, cx)`.
- `repo_color.rs`: `dot` (8 px), `set` (updates the global, saves in the
  background, refreshes windows), `menu_items` ("Automatic" + the ten
  colours, the pick checked).
- Dots: queue rows (before the repo name), Issue / PR tabs (after the state
  icon), the pinned-repos menu, the queue's repo filter (button and menu),
  the repo header. "Colour" submenu (per pinned repo) at the end of the
  pinned-repos menu; a "Colour" button next to Pin in the repo header.

Tests: every repo colour >= 3:1 on bg, surface, selected and menu in all
eight themes and >= 15 degrees from the accent; the name hash is stable and
spreads 50 fake names over >= 8 hues; a pick overrides and clears; DB
`set_color` / `colors` and the 0-9 check; a pick survives a repo re-sync
(`fl-sync` `repo_metadata_pages_starred_and_etag`); UI: a pick shows at
once and is saved, "Automatic" clears it. Clippy passes; `cargo test
--workspace` 258 pass.

Checked by eye (PrintWindow, release, `target/tmp/g2/shots.sh` with repo 1
set to Pink; images `target/tmp/g2s3-*.png`): Midnight Inbox, repo header
and PR tab; Paper Inbox and repo header. Not checked by eye: the opened
menus (pinned "Colour" submenu, repo header menu, repo filter menu); they
need a click, and keystrokes / clicks are not sent to the real window.

Open items from step 3:

- In a narrow queue pane the dot (14 px with its gap) truncates long author
  logins sooner.
- Midnight's Blue repo dot (moved to 213 degrees) is still close to the
  indigo unread dot; a larger gap would push it onto Cyan.
- Home and Search rows show a repo name without a dot (the plan lists only
  queue rows).

Done (step 4, accent derivation, 2026-11-19):

- `crates/fl-app/src/theme/derive.rs`: `Accent { hue, muted, tint }` and
  `derive(accent, dark) -> Palette` (pure). Setting `accent`:
  `{"hue":0-359,"chroma":"normal"|"muted","tint":true|false}`; missing or
  malformed (also hue >= 360, a non-integer hue, an unknown chroma, a
  missing key) = indigo 268, normal, tinted. The palette's `name` is
  `"accent"`.
- Targets in OKLCH from the "Custom accent" board (dark: bg 0.19, surface
  0.165, raised 0.245, border 0.27, selected 0.29 at chroma 0.06, text 0.94,
  muted 0.71, accent 0.70 at 0.16, accent text 0.78; light: bg 0.995,
  surface 0.972, raised 0.935, border 0.91, selected 0.95 at 0.035, text
  0.22, muted 0.50, accent 0.55 at 0.17, accent text 0.50). Not on the
  board, chosen here: control / hover / border-strong / text-strong / body /
  read / faint, diff hunk, gutter, comment and punctuation; muted chroma
  (accent 0.09 dark / 0.10 light, selection 0.035 / 0.02). Tint off sets the
  surface and text chroma to 0 (selection and accent keep theirs). Colours
  outside sRGB lose chroma (binary search) and keep L and hue.
- Contrast pass (`fit`): a token that misses its rule moves its OKLCH L away
  from the backgrounds in 0.005 steps. Buttons: dark mode = near-black text
  (L 0.2) on the accent; light mode = white text, fill darkened until
  >= 4.5:1. GitHub states, `added` / `removed`, star and the six coloured
  syntax tokens start from Midnight / Daylight and move in HSL lightness
  (`fit_hsla`, hue kept). `neutral` = muted text on `raised`. Repo dots:
  Midnight / Daylight saturation, lightness moved until all ten hues (after
  the 15 degree gap from the accent) pass on bg, surface, selected, control.
  Yellow / amber accents in light mode end up brownish: the 3:1 / 4.5:1
  rules force it.
- `Choices` now holds `Palette` values, not `&'static`;
  `Choices::from_settings(dark, light, accent)`. `{"kind":"accent"}` loads
  `derive(Accent::from_setting(accent), mode)`. `main.rs` reads `accent` in
  the same DB read as `theme` / `colors_*`. `luminance` / `contrast` are no
  longer test-only. No picker (step 5): set the rows with `sqlite3`.

Tests: the contrast tests in `theme/tests.rs` now run over the eight themes
plus `derive` for every hue in steps of 5, both modes, both chroma levels,
both tint settings (576 palettes): text, muted, read, body, accent, accent
text, button text, states, pills, diff, syntax on every code background,
avatars, label hues, repo dots and their accent gap. New: `accent` parsing
and fallbacks, `{"kind":"accent"}` loads the derived palette (and indigo
without a setting), the derived accent keeps its hue, muted is calmer, tint
off is grey. Clippy passes; `cargo test --workspace` 261 pass.

Checked by eye (PrintWindow, release, `target/tmp/g2/accent.sh <hue>
<normal|muted> <true|false> <dark|light> <views>`; images
`target/tmp/g2s4-*.png`): indigo 268 dark Inbox + PR, green 150 light Inbox
+ PR, red 25 muted untinted dark Inbox, amber 75 light Inbox, teal 185 muted
dark PR.

Open items from step 4:

- A green accent (about 140-160) looks like the `open` state colour, a red
  one (about 15-35) like `closed`; states keep their hues by design.
- Not measured: the cost of `derive` at startup (about 600 contrast checks;
  expected well under 1 ms). Step 7's bench covers it.

Done (step 5, picker, 2026-11-20):

- `crates/fl-app/src/appearance.rs`: the "Appearance…" dialog. Mode
  (System / Light / Dark), "Colours for" Dark mode / Light mode (the mode
  being edited is shown in the app meanwhile), theme cards for that mode
  (a small picture of the app in the theme: rail, selected unread row,
  read row, button) plus a "Custom accent" card, the 12 board presets
  (`PRESETS`; swatch colour OKLCH 0.66 / 0.16, `theme::swatch`), the hue
  slider (kit `Slider`, 0-359), "Muted colours" and "Tint backgrounds"
  switches, and the readout `Text n:1 · muted text n:1 · accent n:1` (the
  edited palette's text, muted text and accent text on its `bg`). Touching
  a swatch, the slider or a switch also picks the accent for the edited
  mode. No dimmed backdrop, so the app behind shows each change.
- Every change writes the `Choices` global and calls `theme::apply` with
  the edited mode. `Esc` / Cancel puts back the choices and the dark /
  light mode shown when it opened. Done shows the chosen mode and saves
  `colors_dark`, `colors_light`, `accent` and `theme` (deleted for
  System) in one background write.
- `theme::Choices` now also holds `accent` and `mode` (the saved `theme`;
  `None` = system). New: `Pick` (`Theme(name)` / `Accent`), `Choices::pick`,
  `set`, `set_accent`, `settings` (the rows to save), `Accent::to_setting`,
  `Palette::title` (display names, "Rosé"). `fl_db::queries::layout::
  delete_setting`.
- Rail: the theme button (moon / sun for the current mode) opens a menu:
  System / Light / Dark (the saved mode checked) and "Appearance…".
  `Workspace::set_mode` applies and saves `theme`; Ctrl+K "Toggle theme"
  uses it too. Ctrl+K has "Appearance…" (`OpenAppearance`).
- `--appearance` launch flag opens the dialog at start (for PrintWindow
  shots; ignored with `--bench`).

Tests: `appearance/tests.rs` (gpui_kit, test window only): the rail menu
sets Light / Dark / System and saves `theme`, its last item opens the
dialog; Ctrl+K "appearance" + Enter opens it; a card, a swatch, the tint
switch and the light-mode tab apply live and save nothing; Esc restores
the old choices and saves nothing; Done saves all four settings and they
load back as the shown `Choices`. `theme/tests.rs`: `Choices` round-trips
through `settings`. Clippy passes; `cargo test --workspace` 267 pass.

Checked by eye (PrintWindow, release, `target/tmp/g2/dialog.sh <name>
<dark|light> <colors_dark> <colors_light> <accent>`; images
`target/tmp/g2s5-*.png`): Midnight dark, Paper light, green 150 accent
dark. Not checked by eye: the opened rail menu (needs a click).

Open items from step 5:

- System mode does not follow a live OS change (as before). The dialog's
  System choice reads the window's appearance once.
- The dialog has no scroll: about 790 px tall in dark mode (two card
  rows); at the window's 500 px minimum height its bottom is cut off.
- The hue slider track uses the kit's slider colours (accent), not a hue
  gradient as on the board.

Done (step 6, fonts, 2026-11-20):

- `crates/fl-app/fonts/<id>/`: 21 bundled fonts with their `OFL.txt`
  (all SIL OFL 1.1, unmodified, so the Reserved Font Names of Plex,
  Source and Monaspace do not apply). Sources: google/fonts `ofl/*`
  (Inter, Geist, IBM Plex Sans, Figtree, Atkinson Hyperlegible Next,
  Source Serif 4, Google Sans, Space Grotesk, Google Sans Flex, Manrope,
  Plus Jakarta Sans, JetBrains Mono, Geist Mono, IBM Plex Mono, Source
  Code Pro, Atkinson Hyperlegible Mono, Space Mono, Google Sans Code),
  Commit Mono v1.143 release (eigilnikolajsen/commit-mono, font licence
  OFL), Monaspace v1.400 variable zip (githubnext/monaspace), Obel Mono
  1.004 (typographic.systems; zip SHA-256 matches the install.sh pins).
- `crates/fl-app/src/fonts.rs`: `UI_FONTS` (11, Inter first = default),
  `CODE_FONTS` (10, Obel Mono first = default), `OBEL_MONO_MASKED`;
  `Fonts { ui, mono, mask_secrets }` global; settings `ui_font`,
  `mono_font` (JSON id, e.g. `"inter"`), `mask_secrets` (default true).
  Unknown ids, a UI font id in `mono_font` (or the other way) and bad JSON
  load the defaults. Only the chosen two fonts are registered at startup;
  the Fonts tab registers the rest (`load_all`).
- `theme.rs`: every text style has an explicit line height (`TextScale`
  trait: `text_body`, `text_secondary`, `text_small`, `text_label`,
  `text_pane_title`, `text_reading_title`). All views use it. Code is
  `TEXT_CODE` 12.5 px in `LINE_CODE` 20 px rows (the kit diff's
  `mono_font_size` is set from it). `theme::apply` writes the fonts into
  the kit theme after the colours, so a palette change never resets them.
- Appearance dialog: tabs Colours / Fonts. Fonts tab: interface font grid,
  code font grid (each name in its own font), a live sample (queue row,
  comment with inline code, diff hunk with a `ghp_` token), the "Hide
  secrets in code" switch (only while Obel Mono is chosen), and a
  scrollable licence list ("Licences" button). Live apply, Esc restores,
  Done saves the colour rows and the three font rows.
- `--appearance-fonts` launch flag opens the dialog on the Fonts tab (for
  PrintWindow shots).

Deviations:

- Masking uses the font Obel Mono Masked (the foundry's build with `ss01`
  on), not the `ss01` feature. DirectWrite does apply `ss01` (test
  `directwrite_applies_ss01`), but GPUI passes font features to every
  child text run and the kit diff / markdown have no hook for code text
  alone: interface fonts would turn on their own `ss01` (Inter's digits).
- Commit Mono: regular only. The release files register one family per
  weight ("CommitMono 400", "CommitMono 700"); bold code text uses
  synthetic bold.
- Exe 49.3 MB -> 66.4 MB (+17.1 MB, above the 8-12 MB estimate; Google
  Sans 5 MB and Google Sans Flex 3.9 MB are the largest).
- Diff code was 10.5 px in 17 px rows (kit rem 13.5 x mono_font_size/16);
  now 12.5 px in 20 px rows. Diffs look larger than in step 5.

Tests: `fonts/tests.rs` (real DirectWrite): table of 21 OFL fonts, unique
ids; fallback for unknown / bad values; settings round trip; only Obel
Mono masks; every font registers its family and a bold weight; every
ASCII letter's ink fits each text style's line box (weights 400 and 600);
Obel Mono Masked draws `ghp_` + 4 characters then one mask glyph, same
advance; `ss01` works. `files/tests.rs`: copying the masked token line
gives the full token; with every UI font, every code font and Obel
unmasked, queue rows, tab titles and diff lines keep their heights (real
text system, `test_support::with_real_text`). `appearance/tests.rs`: font
picks apply live, Esc restores them, Done saves them. Clippy passes;
`cargo test --workspace` 280 pass.

Bench (release, fake data, `target/tmp/g6/fontbench.sh <ui> <mono>`):

- Cold first launch after build: 891 ms (494 ms before `main`: the larger
  exe loads from disk once).
- Quiet machine, interleaved with the step 5 exe: `app_startup_ms` base
  46.5 / 72.5 / 59.3, new 72.1 / 66.8 / 52.9; launch base 424 / 548 /
  437, new 499 / 496 / 408. The steady cost is the first frame
  (`workspace_built` -> `window_open` 19-22 ms, base 9-11 ms): shaping
  with Inter's 870 KB variable font and Obel Mono.
- `diff_scroll_fps`, default fonts, quiet machine: 240 / 238 / 240.
  Widest code font (Atkinson Hyperlegible Mono, 5 % wider than the
  others): interleaved with base and default on a busy machine, 195 /
  68 / 51 vs base 165 / 35 / 80 and default 193 / 31 / 36; no measurable
  cost, but not repeated on a quiet machine.

Checked by eye (PrintWindow, `target/tmp/g6/shot.sh`; images
`target/tmp/g6/g2s6-*.png`): Fonts tab dark and light (defaults), Source
Serif 4 + JetBrains Mono (switch hidden), a PR in light mode; diff frames
`target/tmp/g6/burst/dark-28.png` (Obel Mono Masked) and `light-17.png`
(Atkinson fonts, same row heights).

Open items from step 6:

- `app_startup_ms` (budget 60) now often misses by about 10 ms (first
  frame shaping). A fix to try: static Inter instances instead of the
  variable font.
- No italic files are bundled (DirectWrite slants the uprights).
- Monaspace Neon (line gap 200) sits slightly low in its picker chip.
- The "Atkinson Hyperlegible Next" chip title is cut off at the dialog
  width.

Done (step 7, checks, 2026-11-20). No code change; this step closes G2.

Screenshots (PrintWindow only, no input sent): one `--bench` run per
setting on `target/tmp/g6/bench-data`, a capture every 150 ms with its
time, then the frame in the middle of the Inbox reading pane scroll and the
one in the middle of the Files tab scroll (picked against the bench marks;
black capture frames skipped). Scripts `target/tmp/g7/` (`run.sh <name>
<dark|light> <colors_json|-> <accent_json|-> <ui> <mono>`, `burst.ps1`,
`pick.py`, `all.sh`, `montage.py`); images `target/tmp/g7/g2s7-*.png`,
Inbox + Files montages in `target/tmp/g7/m/`. Checked by eye:

- All 8 themes (Midnight, Graphite, Fjord, Forest, Rosé, High contrast,
  Daylight, Paper): Inbox queue, reading pane, PR header, Files tab with
  syntax colours and add / remove rows.
- 3 accents: red 25 dark, green 150 light, cyan 210 muted untinted dark.
- All 21 fonts, in 11 runs pairing one interface font with one code font
  (Inter + Obel Mono in the Midnight run; the others alternate dark /
  light). No clipped text, rows keep their heights.
- Seen, not changed: with wide code fonts (Atkinson Hyperlegible Mono,
  Space Mono, Monaspace Neon) the queue row's author login truncates
  sooner (step 3's narrow-pane item). Diff hunk headers (`@@ ... @@`) use
  the interface font (the kit draws them).

Bench (release, fake data, `target/tmp/g7/quietbench.sh`: 5 interleaved
rounds of `main` (pre-G2, `1d8c6e5`), G2 (`f1c06da`) and a test build with
static Inter; then 3 rounds of the step 5 exe, G2 + Atkinson Hyperlegible
Mono and G2 default fonts; raw logs `target/tmp/g7/logs/`). Machine idle
apart from the runs, but noisier than step 1: `main`'s own `launch_ms` was
521-683. The first run of each round is the slowest.

| | `main` | G2 |
|---|---|---|
| `launch_ms` (600) | 521-683, 3 of 5 miss | 549-615, 1 of 8 miss |
| `app_startup_ms` (60) | 45-74, 3 of 5 miss | 62-123, 8 of 8 miss |
| `switch_tab_ms` (8) | 4.0-4.1 | 4.0-4.8 |
| `store_ready` -> `workspace_new` | 23-31 (46 first run) | 24-28 (41-59 in 3 runs) |
| `workspace_built` -> `window_open` | 10-12 (31 once) | 23-26 (64 once) |

- `launch_ms` and `switch_tab_ms` stay in budget. `store_ready` ->
  `workspace_new` does not change: theme apply, `derive` and the font
  choice cost nothing measurable.
- `app_startup_ms` misses in every G2 run: by 2-18 ms warm, 63 ms in the
  first run (median about 65).
  The whole gap is the first frame (`workspace_built` -> `window_open`,
  +13 ms), as in step 6.
- Static Inter (4 instances, opsz 14, wght 400 / 500 / 600 / 700, made with
  fontTools from the bundled file): `workspace_built` -> `window_open`
  22-24 ms, `app_startup_ms` 62-82. No gain, so the variable font is not
  the cause and the shipped fonts are unchanged.
- `diff_scroll_fps` with the widest code font: Atkinson 86 / 212 / 192,
  G2 default fonts 193 / 178 / 193, step 5 exe 30 / 186 / 166 (first run
  of each round noisy). No cost from the wide font. All below step 1's
  240 on this machine today, `main` included (145-238).
- Other budgets in all warm runs: `open_cached_item_ms` 9-10,
  `search_p95_ms` 15-22.

Clippy passes; `cargo test --workspace` 280 pass (no code change).

G2 summary: the look refresh, eight themes, repository colours, accent
derivation, the Appearance dialog and 21 bundled fonts are done
(steps 1-6, PR #9). Plan: `docs/design/themes-plan.md`.

Open decisions (for the user):

1. `app_startup_ms` (budget 60) now misses by about 5 ms warm. Options:
   a) raise the budget to 75 and note the bundled fonts as the reason;
   b) profile the first frame (DirectWrite font face creation and glyph
   rasterisation for in-memory fonts are the suspects, since static
   files do not help); c) register the fonts after the first frame and
   paint it with the system font (a visible font swap).
2. The step 1-6 open items below are not done; pick which go into G3.

Open items carried from G2:

- Dock tabs square (needs a custom `DockAreaRenderer`); action buttons
  24 px vs the board's 30 px; diff add / remove backgrounds from the kit.
- System mode does not follow a live OS change; the title bar follows the
  OS again after an OS app-mode switch.
- Appearance dialog: no scroll (cut off below about 790 px), hue slider
  without a hue gradient, Atkinson Hyperlegible Next chip title cut off,
  Monaspace Neon sits low in its chip.
- Repo dots: narrow queue truncates logins sooner; Midnight Blue close to
  the unread dot; Home and Search rows have no dot.
- Green / red accents look like the open / closed state colours.
- No italic font files; Commit Mono bold is synthetic.
- Not checked by eye: opened menus (rail theme menu, pinned "Colour"
  submenu, repo header and repo filter menus); they need a click.

Next: merge PR #9 after review, then settle the open decisions above.
