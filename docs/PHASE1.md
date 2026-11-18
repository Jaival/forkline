# Phase 1 plan

Phase 1 turns the Phase 0 shell into a working GitHub client. Five agents work
in parallel, each in its own git worktree branched from `phase-1`. The
integrator (main thread) merges them into `phase-1` in agent order, resolves
conflicts in shared files, regenerates bindings and runs `bun run check`.

Read `docs/HANDOFF.md` first: its shared rules (stack, paging, `data-changed`,
schema rules, bench rules) still apply.

## Decisions

- **Auth:** GitHub OAuth device flow. Client ID comes from the build-time env
  var `FORKLINE_GITHUB_CLIENT_ID` (`option_env!`), no client secret. Token is
  stored in Windows Credential Manager (`keyring` crate). In dev,
  `FORKLINE_GITHUB_TOKEN` overrides the stored token (see `fl_sync::auth::token`).
- **Sync scope:** metadata for every repo the user can access (name, owner,
  description, stars, `starred`, `pinned` stays local). Deep sync (issues,
  PRs, comments, labels, reviews, threads, checks, commits) only for repos
  with `pinned = 1`. Notifications for all repos.
- **Write actions:** see `fl_types::Action`. All go through
  `action_enqueue`, are optimistic, and use `pending_fields` (rules in
  HANDOFF.md).
- **Git:** the `git` CLI (spawned with `std::process::Command`) for blobless
  bare clones, fetch, merge-base and diffs. Git for Windows is a stated
  requirement. No gitoxide.

## Shared contracts (fixed; change only via the integrator)

| Contract | Location | Producer | Consumers |
|---|---|---|---|
| `Action`, `ReviewEvent`, `MergeMethod`, `DraftReviewComment`, `DiffSide` | `crates/fl-types/src/action.rs` | agent 1 | agents 3, 4 |
| `DiffSummary`, `DiffFile`, `DiffHunk`, `DiffLine` | `crates/fl-types/src/diff.rs` | agent 5 | agent 4 |
| `GitStore` (`ensure_commits`, `diff_summary`, `diff_file`) | `crates/fl-git/src/lib.rs`, `AppState.git` | agent 5 | agent 4 |
| `index_issue`, `search`, `SearchHit` | `crates/fl-search/src/lib.rs` | agent 5 | agent 1, fl-fake |
| `fl_sync::auth::token()` | `crates/fl-sync/src/lib.rs` | agent 1 | agents 4, 5 |
| `repos.starred` | migration 0003 | agent 1 (sync) | agent 2 |
| settings key `viewer` = `{"id":n,"login":"..."}` | `settings` table | agent 1 | agents 2, 3 |

Keep contract signatures as they are. If you need a change, stop and report it
instead of editing the contract.

## File ownership

Each agent edits only its own files. Shared files allow small, local edits:

- `app/src/lib.rs`: only your group in `collect_commands!` (and agent 1 may
  add a sync field to `AppState` and start the engine in `setup`).
- `crates/fl-types/src/lib.rs`: none. Put DTOs in your module
  (`sync.rs`, `repo.rs`, `lists.rs`, `pr.rs`, `search.rs`) and remove that
  module's name from the `#[allow(unused_imports)]` line once it has types.
- `crates/fl-db/src/migrate.rs`: append one line for your migration file.
  Numbers: 0004 agent 1, 0005 agent 2, 0006 agent 3, 0007 agent 4, 0008 agent 5.
  Migrations are additive only (new tables, columns, indexes).
- `crates/fl-fake/src/main.rs`: none. Use your seed module
  (`repo.rs` agent 2, `lists.rs` agent 3, `pr.rs` agent 4). Agent 5 may
  change the base loop to call `fl_search::index_issue`.
- `ui/src/shell/ViewHost.tsx`: only your `case` lines.
- `ui/src/lib/layout.ts`: add a `ViewRef` kind only if your area needs one.
- `ui/src/bindings.ts`: regenerate with `bun run bindings`; never hand edit.
  The integrator regenerates it after each merge.
- UI code goes in `ui/src/features/<area>/`. Shared UI helpers you need
  (e.g. a virtual list, markdown render) go in `ui/src/lib/` only if your
  brief says you own them.

`data-changed` scopes: `issues`, `repos`, `inbox`, `pr`, `sync`, `search`.
Query keys start with the scope.

## Bench

Each agent with a budget in `bench/budgets.json` adds a scenario in
`ui/src/features/<area>/bench.ts` that exports
`async function bench(mark: (name: string, value: number) => Promise<void>)`
and measures its number with `performance.now()` and `signalReady`. The
integrator wires scenarios into `ui/src/bench.ts` and `fl-bench`.

## Definition of done (every agent)

- `bun run check` passes in your worktree (tsc, eslint, vitest, clippy
  `-D warnings`, cargo tests).
- Every new list query has an index; extend the `query_plans` test in
  `crates/fl-db/tests/schema.rs` for new queries.
- Rust tests for backend logic; Vitest for pure UI logic.
- Views work on fake data: `bun run fake`, then `bun run dev -- -- --data-dir fake-data`
  (or the equivalent) shows real content, not placeholders.
- Commit on your worktree branch with a clear message. Do not push. Do not
  merge. Report: what you built, contract questions, what is not done.

## Agent briefs

### Agent 1: sync, auth, action queue (Opus)

Owns `crates/fl-sync`, `app/src/commands/actions.rs`, `crates/fl-db/src/queries/sync.rs`,
`fl-types/src/sync.rs`, migration 0004, `ui/src/features/auth/`, `ui/src/shell/Footer.tsx`.

- HTTP client: `reqwest` (rustls) on tokio (Tauri's runtime). GraphQL for
  deep sync, REST for notifications and where GraphQL lacks data.
- Device flow sign-in: commands to start (returns user code + verification
  URL), poll, sign out, and read auth state. Sign-in UI in
  `features/auth/`, shown when commands return `CmdError::Unauthorized` or
  no token exists. Open the URL with the opener plugin.
- Write `viewer` setting after sign-in.
- Sync engine: background task. Repo metadata list, then deep sync of
  pinned repos (incremental via `updated_at` / `since` and `sync_state`
  cursors and ETags), notifications polling honoring `X-Poll-Interval`.
  Respect rate limits (primary and secondary); back off and show it in
  `SyncStatus`. Pinning a repo triggers its deep sync soon.
- Upserts must not overwrite fields listed in `pending_fields`.
- Call `fl_search::index_issue` for every issue whose title, body or
  comments changed, in the same transaction.
- Emit `data-changed` after each batch with the touched scopes.
- Action queue: `action_enqueue(Action)` validates, applies optimistically,
  writes `action_queue` + `pending_fields`, emits `data-changed`, returns
  the id. A worker sends due actions, retries with backoff on network and
  5xx errors, fails at once on 4xx validation errors, rolls back per the
  HANDOFF rules and keeps the failed row visible (`SyncStatus.failed_actions`).
  Commands to retry / discard a failed action.
- Footer: sync state, rate limit pause, pending/failed actions.
- Tests: mock GitHub with a local HTTP server (e.g. `wiremock` or a small
  axum/hyper test server). No test may call real GitHub.
- Bench: `optimistic_action_network_wait_ms` = time from enqueue to the
  optimistic row being visible must not wait on the network (measure in Rust
  test or bench scenario).

### Agent 2: repos, sidebar, keymap (Sonnet)

Owns `app/src/commands/repo.rs`, `crates/fl-db/src/queries/repo.rs`,
`fl-types/src/repo.rs`, migration 0005, fake `repo.rs`,
`ui/src/features/repo/`, `ui/src/shell/Sidebar.tsx`, `ui/src/shell/keymap.ts`,
`ui/src/features/palette/`.

- Repo overview view (`kind: "repo"`): description, language bar
  (`languages_json`), stars/forks, default branch, license, open PR / issue
  counts, recent commits on the default branch with check state, latest
  releases, top contributors, star history sparkline. Links to the repo's
  PR and issue lists (open those views filtered to the repo; agree on the
  `ViewRef` with agent 3 by using `{ kind: "pulls" | "issues", repoId? }`
  — agent 3 owns those views; you only navigate).
- Repositories view (`kind: "repos"`): all known repos, paged, filter by
  name, pin / unpin toggle (local write: `repo_set_pinned`, emits `repos`).
- Starred view (`kind: "starred"`): repos with `starred = 1`.
- Sidebar: pinned list with counts, drag to reorder is out of scope.
- Keymap: full global keymap. `j` / `k` move selection in the active list,
  `Enter` opens, `/` focuses search, `Ctrl+K` opens a command palette
  (navigate to views, pinned repos, recent items), `Ctrl+Tab` switches tabs.
  Provide a small list-selection hook in `ui/src/lib/listNav.ts` that agents
  3, 4, 5 can use; keep its API minimal and documented.
- Bench: `switch_tab_ms` (tab switch to painted, budget 16 ms).

### Agent 3: lists, inbox, issue detail (Sonnet)

Owns `app/src/commands/{lists,inbox,issue}.rs`,
`crates/fl-db/src/queries/{lists,inbox,issue}.rs`, `fl-types/src/lists.rs`,
migration 0006, fake `lists.rs`, `ui/src/features/{home,lists,inbox,issue}/`,
`ui/src/lib/html.ts`, `ui/src/lib/VirtualList.tsx` (shared virtual list).

- `issue_list(filter, page)`: Pull requests, Issues and Review requests
  views; filter by repo, state, author, assignee, label, `is_pr`,
  review-requested-for-viewer; sort by updated. Paged ≤ 100, indexed.
- Inbox (`kind: "inbox"`): notifications grouped by repo or flat, unread
  state, mark read / done via `Action::NotificationRead` / `NotificationDone`,
  open the linked issue / PR.
- Issue detail: body and comments rendered as sanitized markdown
  (markdown-it + DOMPurify, task lists), labels, assignees, timeline events,
  linked PRs. Actions: comment, close / reopen, edit labels, edit assignees,
  all via `action_enqueue`. Show pending state for rows in `pending_fields`
  and local comments with negative ids.
- Lists use TanStack Virtual through a shared `VirtualList` component.
- Bench: `open_cached_item_ms` (click to issue detail painted, 50 ms).

### Agent 4: PR view, diff viewer (Sonnet)

Owns `app/src/commands/{pr,diff}.rs`, `crates/fl-db/src/queries/pr.rs`,
`fl-types/src/pr.rs`, migration 0007, fake `pr.rs`, `ui/src/features/pr/`,
`ui/src/features/diff/`.

- PR view (`kind: "pr"`): header (branches, draft, mergeable, review
  decision, check state), conversation (reuse agent 3's markdown helper from
  `ui/src/lib/html.ts`; do not edit it), reviews, checks list, commits, and
  a Files tab.
- Actions: submit review (with inline draft comments), reply / resolve
  threads, merge with method, via `action_enqueue`.
- Diff: `diff_summary(pr_id)` and `diff_file(pr_id, path)` commands. Look up
  the repo and `pr_details` oids, call `GitStore::ensure_commits` then
  `diff_summary` / `diff_file` on the blocking pool. Until agent 5 lands,
  `GitStore` returns errors: show a clear "diff not available" state, and
  build the viewer against a local fixture (`DiffFile` JSON in tests).
- Diff viewer: file tree, unified view, virtualized lines (60 fps on a
  10k-line diff), syntax-free is fine, review threads inline at their
  lines, mark file viewed (`pr_viewed_files`, resets on head change).
- Bench: `diff_scroll_fps` scenario using a large fixture diff.

### Agent 5: git and search (Sonnet)

Owns `crates/fl-git`, `crates/fl-search`, `app/src/commands/search.rs`,
`fl-types/src/search.rs`, migration 0008, `ui/src/features/search/`, and the
base loop's search indexing in `crates/fl-fake/src/main.rs`.

- `fl-git`: implement `GitStore` with the `git` CLI: bare blobless clone
  (`--filter=blob:none`) under `<data_dir>/git/<owner>/<name>.git`, fetch
  missing oids (PR heads via `refs/pull/<n>/head`), merge-base, tree diff,
  per-file unified hunks with line numbers (parse `git diff` output). Pass
  the token per command via `-c http.extraHeader=...` or a credential
  helper reading an env var; never put it in argv, URLs, git config, logs or
  disk. Hide console windows (`CREATE_NO_WINDOW`). Lock
  per repo so two fetches do not race. Tests on a local repo made in a temp
  dir (no network).
- `fl-search`: `index_issue` and `search`. Parser for free text plus
  `is:pr`, `is:issue`, `is:open`, `is:closed`, `is:merged`, `author:`,
  `label:` (quoted values), `repo:owner/name`, `assignee:`, negation `-`.
  FTS5 query escaping so user text never breaks the query. Rank by bm25,
  group by issue, snippet with the U+E000 / U+E001 markers. Unit tests for
  the parser and for search on a seeded DB.
- `search(query, page)` command and Search view (`kind: "search"`), plus
  the global search box that `/` focuses (agent 2 wires the key; you expose
  a focus target id `global-search`).
- Bench: `search_p95_ms` (30 ms p95 over a fixed query set on fake data).
