# G4 plan: finish the G3 workflow items

Source: G3 open items (`docs/HANDOFF.md`, "G3 wrap-up (2026-11-27)") and
D12 / D13 in `docs/design/g3-workflow-review.md`. Order and open questions
to agree with the user before any code. One step per branch and PR. Each
step ends with `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, and a bench run when it touches rendering (AC
power, no other `forkline*` / `fl-bench` process, alternate with a `main`
build; every budget in `bench/budgets-gpui.json` must pass).

Build with `CARGO_TARGET_DIR=C:/Users/saija/cargo-target/<branch>`.

Not in G4: multi-account (own phase, plan first), code reuse (G5,
`docs/design/g5-reuse-plan.md`).

## Proposed order

| Step | Item | Why here |
|---|---|---|
| 1 | Explore note engine test (item 4) | Small, no UI. Warms up the wiremock setup used in step 2 and 4. |
| 2 | "Done all" failure feedback (item 3) | Bug fix. Adds `Engine::enqueue_all`, which step 4 reuses. |
| 3 | "New since last view" line (item 2, D13) | Read-only UI, no GitHub writes. |
| 4 | Batch merge of green bot PRs (item 1, D12) | Writes to GitHub: needs the rules below agreed first. |
| 5 | Frame-rate gap (item 5, optional) | Measure only; a fix only if the cause is clear. Last so it measures the G4 code too. |

Steps 1 and 2 may share one PR if the user prefers fewer PRs.

## Step 1: engine test for the Explore note

Today (`crates/fl-sync/src/engine.rs:360-371`): an Explore error sets
`Runtime::note` only if `Runtime::noted` did not hold the same text; an
Explore pass that succeeds clears `note`. No test covers it.

Build:
- A test in `crates/fl-sync/tests/sync.rs`: a wiremock server answers the
  Explore request with 403 (body like the `buttercup` OAuth App restriction).
  - Pass 1: `SyncStatus::note` is `Some("Explore: …")`, `error` is `None`.
  - Pass 2, same 403: `note` is `None` (shown once per launch).
  - A different error text: shows once.
  - Explore 200: `note` is `None`.
- Explore runs only when `repos_due`; the test needs a way to force a repo
  tick (a test hook or set the stored `repos` state to an old time).

Tests: the new test. Bench: none (no rendering).

Open: none.

## Step 2: "Done all" failure feedback

Today (`crates/fl-app/src/views/queue.rs:298-319`, `flush_done`): a loop of
`engine.enqueue(Action::NotificationDone { id })`, one DB write and one
`emit` per row; `?` stops at the first error; the task is `detach`ed, so
the error is dropped. The rows come back on the next reload with no word.

Send failures (the worker's DELETE fails) are a separate path: each one is
a failed action in the footer toast and the failed list (Retry / Discard).

Build:
- `Engine::enqueue_all(Vec<Action>) -> Result<EnqueueAll>`
  (`crates/fl-sync/src/engine.rs`, `queue.rs`): one write transaction, a
  savepoint per action so one bad action does not undo the others, one
  `emit` with the union of scopes, one `wake_actions`. Returns the ids
  that went in and the count (and first error text) of those that did not.
- `flush_done` uses it, keeps the task, and on a partial failure shows a
  toast: "Marked 48 of 50 done. 2 failed: <first error>". Rows that failed
  come back at once (reload), not on a later reload.
- Send failures: N failed `NotificationDone` actions after one "Done all"
  would fill the failed list with N rows. See open question Q2.

Tests:
- fl-sync: `enqueue_all` with one invalid action (e.g. an unknown thread
  id) enqueues the rest and reports one failure; one `emit`.
- fl-app (gpui_kit test, `views/queue/tests.rs`): "Done all" with one row
  that cannot be enqueued shows the toast text and brings that row back.

Bench: Inbox is not in the bench set; run it anyway (toast code is shared
with the footer).

Open:
- Q1. Toast text and where: the footer toast (step 6 of G3), or the
  Inbox's own "N marked done · Undo" bar? Proposal: the footer toast, so it
  outlives the Inbox tab.
- Q2. Send failures of one "Done all": leave one failed row per
  notification (today's rule; the failed list has no Retry all /
  Discard all), or group
  them as one row "Done all: 2 of 50 failed" with Retry / Discard for the
  group? Proposal: keep one row per action, add "Retry all" / "Discard all"
  to the failed list.

## Step 3: "New since last view" line (D13, second half)

Today: opening an unread item calls `mark_viewed` (`views/reading.rs:240`,
`fl_db::queries::{issue,pr}::mark_viewed`), which overwrites
`last_viewed_at`. The old value is lost before the conversation draws.
`RowReason` (`fl-types/src/queue.rs`) already counts new comments and
commits from `last_viewed_at` in SQL (`fl-db/src/queries/queue.rs:69-75`).

Build:
- `read_detail` returns the `last_viewed_at` it read (before the write).
  `ReadingPane` keeps it per item as `seen_at` for as long as the item is
  on show; reopening it later uses the new value (so the line moves).
- `Conversation::set(detail, seen_at)` (`views/reading/conversation.rs`):
  entries are sorted by time; insert one divider entry before the first
  entry with time `> seen_at`: "New since your last view · 3 comments,
  1 review". No divider when `seen_at` is NULL (never opened) or nothing is
  newer.
- On open, scroll to the divider when it exists (else keep the top).
- Commits / Checks tabs: a dot on the tab label when a commit / check run
  is newer than `seen_at` (see Q4).

Files: `views/reading.rs`, `views/reading/conversation.rs`,
`views/reading/tabs.rs`, `fl-db/src/queries/{issue,pr}.rs` (if the read
needs a column), tests in `views/reading/tests.rs` and the unit tests at
the end of `conversation.rs`.

Tests:
- Unit: divider position for: never viewed, nothing new, new comment
  between old ones (by time), untimed reviews (they sort last today).
- gpui_kit: open an unread PR -> divider shows; the DB `last_viewed_at` is
  updated; reopen -> no divider.

Bench: `open_cached_item_ms` (budget 30 ms) and `diff_scroll_fps`. The
bench DB is read-only (`mark_viewed` is skipped); seed one fake item with
`last_viewed_at` set so the divider path is measured.

Open:
- Q3. Scroll to the divider on open, or keep the top and only show the
  line? Proposal: scroll to it (the point of the line).
- Q4. Dots on Commits / Checks tabs: in this step, or skip? Proposal:
  include; both counts come from data already cached.
- Q5. Does "new" count the user's own comments? Proposal: no (own comments
  and own reviews never count as new; the `RowReason` SQL counts them today,
  fix both the same way, or leave `RowReason` as is).

## Step 4: batch merge of green bot PRs (D12)

Today: Merge is one PR at a time from the action bar (`Action::Merge`,
`fl-sync/src/queue.rs:461`, GraphQL `mergePullRequest`), with a method
menu and a confirm. `merge_state` blocks drafts, conflicts and pending
fields; it does not look at checks. Bot = avatar URL has `/in/` (D3).
Home's "Waiting on me" group already holds bot PRs on my repos with a
"bot" tag.

Proposed rules (to agree, Q6-Q11): a PR is in the batch only if all are
true at click time, from cached data:
1. Open, not draft, author is a bot (avatar `/in/`), repo owned by the
   viewer or `repos.member`.
2. `check_state` is Success (every check run passed or was skipped /
   neutral). Pending, failure, or no checks = not in the batch.
3. `mergeable` is `MERGEABLE` (not `CONFLICTING`, not `UNKNOWN`).
4. `review_decision` is not `CHANGES_REQUESTED`.
5. No pending field (no other action waiting for the PR).
6. Synced in the last N minutes (Q9), so the state is not stale.

Build:
- `fl_db::queries::queue::green_bot_prs(conn, query) -> Vec<(IssueId,
  MergeMethod)>` with the rules above in SQL; method = the repo's last
  method (`repo::merge_method`) or Q8's default.
- Button "Merge all green (N)" on the group header (Q7). Click opens a
  confirm dialog: the PR list (repo, number, title, method), "Merge N
  pull requests", Cancel. Nothing is sent before Confirm.
- Confirm: `Engine::enqueue_all` with one `Action::Merge` per PR (step 2).
  The worker sends them one at a time, oldest first, as today.
- Order inside one repo: after one merge, GitHub may mark the next PR
  conflicting (lockfiles). Its merge then fails, rolls back, and shows in
  the failed list. See Q10.
- Result: one toast, "Merged 5 of 7. 2 failed: <reason>", when every
  action of the batch has finished.

Files: `fl-db/src/queries/queue.rs`, `fl-sync/src/engine.rs`,
`views/queue.rs` (header button), a confirm dialog (`views/queue/` or
reuse the action bar's confirm style), `fl-types` (batch result if
needed).

Tests:
- fl-db: `green_bot_prs` keeps only rows that pass all rules (one fixture
  row per failed rule).
- fl-sync (wiremock): 3 merges, the second returns 405 "not mergeable":
  1 and 3 merge, 2 rolls back to open, one failed action.
- gpui_kit: the button shows with the count; Cancel sends nothing; Confirm
  enqueues N merges.
- First real use: on the user's account, one batch on 2 dependabot PRs, by
  the user, with a check of each on github.com.

Bench: Home list (`open_home_ms`, no budget) and every budget.

Open (all before code):
- Q6. Which bots: any GitHub App author (`/in/`), or a list (dependabot,
  renovate, github-actions)? Proposal: any App author; the confirm dialog
  lists each PR, so the user sees who.
- Q7. Where: Home "Waiting on me" header, Pull requests (a "Bots" group,
  P5), or both? Proposal: Home only (bot PRs already gather there).
- Q8. Merge method: each repo's last used method, else "Squash"? Or one
  method picked in the dialog for the whole batch? Proposal: one picker in
  the dialog, default = repo's last method, else Merge commit (GitHub's
  default).
- Q9. Staleness: block the batch when the PR was synced more than 10 min
  ago, or sync those PRs first, then show the dialog? Proposal: refresh the
  PRs (one GraphQL call per repo) when the dialog opens; the list updates
  in place.
- Q10. Lockfile conflicts after the first merge: accept the failure (user
  merges or asks dependabot to rebase), or post `@dependabot rebase` on
  failure? Proposal: accept the failure in G4; no comments posted for the
  user.
- Q11. Required checks only, or all checks? GitHub's branch protection
  decides the required set; we do not sync it. Proposal: all check runs
  must pass (stricter, no new API).
- Q12. Approve before merge (some repos require a review)? Proposal: no;
  if branch protection rejects, the merge fails and shows in the toast.

## Step 5 (optional): frame-rate gap after the G3 step 5 rebase

Facts (HANDOFF, G3 step 5 section): after the rebase, `animation_fps`
163-221 (base 205-236, lowest in palette or dialog), diff scroll 210-240
(base 236-240). Every budget passes (120 fps). Step 5 added Home as the
launch view, `RowReason` SQL, the footer note, Home's `repos` listener.

Plan:
- Reproduce on `main`: 5 runs, alternate with the `g3-animations` build.
- Suspects, cheapest check first:
  1. Home is now open (a hidden tab) during the bench: a `DataChanged`
     listener re-ranking Home in SQL, or a timer. Check: bench with
     `open_home = false`.
  2. Inbox `READ_AFTER` timer or the Undo timer waking frames.
  3. More elements per frame in the sidebar / footer (Home entry, note).
- Tool: per-frame timings from the bench report (frame time histogram),
  not guesses. Fix only if one suspect explains the gap; else record the
  result in HANDOFF and close.

Open:
- Q13. Do this step at all? Proposal: yes, time-boxed (half a day), since
  later steps add to Home and the queue too.

## Questions for the user (summary)

- Order: 1 -> 2 -> 3 -> 4 -> 5? Steps 1 and 2 in one PR?
- Q1-Q2 (Done all), Q3-Q5 (new since last view), Q6-Q12 (batch merge),
  Q13 (frame rates).

## Decisions (2026-11-27, agreed with the user)

- D1. Order 1 -> 2 -> 3 -> 4 -> 5. Steps 1 and 2 ship in one PR.
- D2 (Q1). A partial "Done all" failure shows in the footer toast:
  "Marked X of N done. M failed: <first error>".
- D3 (Q2). Send failures stay one failed-list row per action. The failed
  list gets "Retry all" / "Discard all"; they act on every failed action
  (no batch id, no schema change).
- D4 (Q3, Q4). The item opens scrolled to the "New since your last view"
  divider. Commits / Checks tab labels get a dot when they hold something
  newer than the last view.
- D5 (Q5). The viewer's own comments, reviews and commits never count as
  new, in the divider and in `RowReason` ("N new comments").
- D6. The divider counts comments, reviews and commits only; label,
  assign and other timeline events do not move it.
- D7 (Q6). Batch = any GitHub App author (avatar `/in/`), on a repo the
  viewer owns or is a member of; open, not draft; every check run passes
  (success / skipped / neutral); `MERGEABLE`; not `CHANGES_REQUESTED`; no
  pending field.
- D8 (Q7). "Merge all green (N)" only on Home's "Waiting on me" header;
  hidden when N is 0.
- D9 (Q8). One method picker in the confirm dialog for the whole batch;
  default = the most common last-used method of the repos, else Merge
  commit.
- D10 (Q9). The dialog refreshes the listed PRs from GitHub when it opens.
  A PR that is no longer green stays in the list greyed out with its
  reason and is left out; the confirm count follows.
- D11 (Q10, Q12). No `@dependabot rebase` comments, no approve before
  merge. A rejected merge (conflict after an earlier merge, branch
  protection) rolls back and shows in the toast and the failed list.
- D12. A failed merge does not stop the rest of the batch, in that repo or
  others. One result toast when every action of the batch has finished.
- D13 (Q11). All check runs must pass (we do not sync the required set).
- D14 (Q13). Step 5 is done, time-boxed to half a day; fix only if one
  cause explains the gap, else record the result in HANDOFF.
