# Handoff (Phase 0 + Phase 1, GPUI G0)

## Workspace ownership

- `crates/fl-types`: shared DTOs and command errors.
- `crates/fl-db`: SQLite connection setup, migration 1, query modules.
- `crates/fl-sync`: GitHub auth (token order: `FORKLINE_GITHUB_TOKEN`, saved
  token in Windows Credential Manager, `gh auth token`; device flow until the
  Tauri app goes), REST/GraphQL client, sync engine, optimistic action queue.
- `crates/fl-git`: `GitStore` over the `git` CLI (bare blobless clones, diffs).
- `crates/fl-search`: FTS5 indexing (`index_issue`) and query parser (`search`).
- `crates/fl-fake`: isolated deterministic fixture data generator.
- `crates/fl-bench`: budget checker (`fl-bench gpui` for the GPUI app).
- `crates/fl-app`: the GPUI app (binary `fl-app`; renamed `forkline` at parity).
- `app`: Tauri startup, typed command registration, and event boundary.
- `ui`: React shell, generated Tauri bindings, and client-side layout state.

## Shared rules

- The UI moves to GPUI + gpui-component on branch `gpui`; see
  `docs/adr/0001-gpui.md` for the stack, pins and changed decisions. The
  Tauri + React app (`app/`, `ui/`) stays until GPUI parity, then is deleted.
  Do not change the stack without asking.
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
  `gpui_layout` (not `ui_layout`, which the Tauri app still owns).
- Keys: `Ctrl+T` new tab, `Ctrl+W` close tab, `Ctrl+\` split right.
  Covered by UI tests in `crates/fl-app/src/workspace.rs`.
- `Store` global (`crates/fl-app/src/store.rs`): `read` runs SQLite on a
  background thread, `run` runs engine futures on the tokio runtime, and the
  store emits `DataChanged { scopes }` for sync and action changes.
- `fl-types`: specta derives behind the `specta` feature (Tauri app only).
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
(B = shell, D = PR Files tab). Steps 1-4 are done; nothing is committed.

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
  (offline: local clone only; online: `ensure_commits` first); `read_only`
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

Next (new thread): step 5, PR Files tab (D layout: file list, gpui-component
`Diff`, thread rail, Viewed state; bench `diff_scroll_fps` >= 120). Then
steps 6-7.

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
