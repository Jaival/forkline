# G3 step 5: workflow review

Date: 2026-11-26. Branch `g3-workflow` (on `g3-font-size`).

## Method

- Data: a `.backup` copy of the real DB (`%LOCALAPPDATA%/Forkline/forkline.db`,
  schema 12, taken read-only). It predates step 1, so the first launch of this
  build on the copy is a true first run: auto-pin, search hits, deep sync.
- Views were opened by writing the `gpui_layout` row and relaunching with
  `--offline`; no input was sent. Click counts come from the code
  (`views/queue.rs`, `views/reading/action_bar.rs`, `sidebar.rs`).
- Captures (PrintWindow, 1400x900): `C:/Users/saija/cargo-target/shots/g3w/`
  (`01-first-launch`, `sync-*s`, `w01-home` ... `w10-repos`).

## The account, after the first sync

| Item | Count |
| --- | --- |
| Repos / pinned (auto) | 627 / 10 |
| Open PRs (all mine or dependabot on my repos, 3 in a foreign repo) | 22 |
| - mine | 13 |
| - dependabot | 9 (one with failing checks, oldest 2026-11-03) |
| Open issues | 5 (3 in `jaivalsaija`, 2 from 2024 in `taskly`) |
| Review requests | 0 |
| Notifications | 132, 0 unread, 0 done; 112 are "Your PRs", most already merged |

A solo developer. Nobody asks for reviews. The review work is the bot PRs in
my own repos, and the "what changed" work is CI and merges on my own PRs.

## Jobs and friction

Clicks are from a fresh launch with the default layout (Inbox tab).

### J0. First run (sign in, first sync)

1. Launch: the old layout (Inbox, Pull requests, Issues). Issues shows
   "Nothing here"; sidebar says "Pin repositories". Footer: "Synced 13 h ago".
2. About 20 s later: 10 repos are pinned without notice. Lists fill in over
   about 2 minutes (30 -> 351 issues).
3. At 50 s: a red Explore 403 (`buttercup` OAuth App restriction) fills the
   footer.

Friction:
- F1. No first-sync progress. "Synced 50 s ago" shows while the deep sync
  still runs; empty lists look broken, not "loading".
- F2. Auto-pin is silent. 3 of 10 pinned repos (JobHunt, jobhunt-agent,
  payably) have 0 open PRs and 0 issues; the user does not learn why these
  repos were picked or how to change the set.
- F3. A 403 from an org the user does not care about is the loudest thing on
  screen during the first run.

### J1. "What needs me now?" (open the app)

Steps: launch -> Inbox (132 old rows) -> Pull requests tab (22 rows) ->
Issues tab (5) -> Review requests (0). 3 clicks and 4 lists to read; no
single answer.

Friction:
- F4. Home exists (`views/home.rs`, "Recent activity") but is not in the
  sidebar or the palette, and it is one flat list by `updated_at`: open items
  of every synced repo, no notion of "needs me".
- F5. Nothing ranks by "my turn": my PR with failing checks, a bot PR with
  green checks ready to merge, an issue assigned to me, a mention. The rows
  carry the check state, so the data is there.
- F6. The launch view is whatever was open last; on first run, Inbox full of
  read, stale rows.

### J2. "What needs my review?"

Steps: Review requests -> "Nothing here". The real work (9 dependabot PRs)
is in Pull requests, mixed with my 13 own PRs.
To list only bot PRs: Pull requests -> `/` -> type `author:dependabot` -> Enter
(1 click + typing; no saved view by default).
Merge one: open (double-click or Enter) -> "Merge commit" in the action bar
(the bar already shows "Checks pass") -> Confirm. 3 actions per PR, plus a
tab to close (`w07-pr-dependabot.png`).

Friction:
- F7. Review requests is empty for a solo account; it gives no hint where the
  review work is.
- F8. No "PRs on my repos by others / bots" list. No grouping by author type
  in Pull requests (one group "Open").
- F9. One PR at a time: no batch approve / merge for bot PRs with green checks.

### J3. "What changed on my PRs?"

Steps: Pull requests -> scan 22 rows for unread dots and the check icon ->
open the PR -> Conversation / Checks / Commits tabs to find what changed.

Friction:
- F10. The filter placeholder says `author:me`, but `author:me` matches a user
  named "me" (`fl-search/src/lib.rs:63`). There is no `@me`. Own PRs cannot
  be filtered without typing the login.
- F11. The unread dot says "something changed", not what: no "new commits",
  "checks failed", "approved", "new comment" marker; the item view has no
  "new since last view" line.
- F12. No review state (approved / changes requested) in the list row.

### J4. "Triage my repo's new issues"

Steps: sidebar pinned repo -> repo page (counts card) -> Open issues card
(1 click) -> list filtered to the repo. Or Issues -> repo dropdown (2 clicks).
Then per issue: open -> `L` labels / Assignees / `X` close.

Friction:
- F13. No "new" or "untriaged" filter: no `no:label`, `no:assignee`, sort by
  created. Sort is `updated_at` only.
- F14. Sidebar counts "1 · 3" have no legend (PRs · issues).
- On this account the job is small (5 open issues, all mine). Low priority.

### J5. "Follow a notification"

Steps: Inbox -> click a row (marks it read on GitHub) -> reading pane ->
`E` done (or `H` snooze). 2 actions per row, one row at a time.

Friction:
- F15. Inbox holds 132 rows nobody will act on: 112 "Your PRs" for PRs that
  are merged or closed. No "Done all", no "done when merged/closed", no
  multi-select.
- F16. Selecting a row (even with j/k) marks it read on GitHub at once.
  Skimming changes remote state.
- F17. 10 notifications (Releases, items not cached) have no item; they show
  a title only.

### J6. Others seen

- F18. Repos page opens with star history and languages first; the open
  PR / issue cards are the only links to work.
- F19. Explore 403 (see F3) repeats on every sync.

## Proposed changes (to agree)

P1. **Home = "Needs me now"**: first sidebar entry and the launch view. Sections
in order, each hidden when empty:
  1. My PRs that need action: checks failed, changes requested, conflicts.
  2. PRs waiting on me: review requests, plus bot PRs on my repos.
  3. Assigned to me / mentions (unread notifications with an item).
  4. My PRs ready to merge (approved or no review needed, checks green).
  5. Recently updated (the current Home list), collapsed.
P2. **First-run flow**: a progress line ("Syncing 4 of 10 repos") in place of
"Synced N ago" until the first deep sync ends; a one-time note "Pinned 10
repos you pushed to recently - Change" that opens Repositories.
P3. **Filter `@me`**: `author:@me`, `assignee:@me`, `review-requested:@me`;
fix the placeholder.
P4. **Inbox clean-up**: "Done" for all notifications whose item is merged or
closed (one action), and "Done all in this group".
P5. **Bot PR batch**: in Pull requests, group "Bots" (dependabot, renovate) and
"Merge all green" for the group.
P6. **What changed**: on a PR row, a short reason ("checks failed", "2 new
comments", "approved") from data already cached.
P7. Small: sidebar count legend (tooltip), Explore 403 shown once per org then
quiet, read-on-select becomes read-on-open.

## Decisions

- D1 (P1, yes). Home becomes "Needs me now": first sidebar entry, the launch
  view. Sections in order, each hidden when empty: my PRs that need action
  (checks failed, changes requested, conflicts); PRs waiting on me (review
  requests + bot PRs on my repos); assigned to me / mentions; my PRs ready to
  merge; recently updated (collapsed).
- D2. Launch: restore the saved tabs, open Home (or reuse its tab) and make it
  active. A setting lets the user keep the last active tab instead
  ("Open Home on launch", on by default; Settings view + palette).
- D3. "Waiting on me" = review requests + any open, non-draft PR by someone
  else on a repo I own or am a member of (bots and humans). Bot rows get a
  "bot" tag. Bot = GitHub App account (avatar URL `/in/<id>`, no schema
  change needed). `pr_details` already has `review_decision`, `mergeable`,
  `check_state` for the other sections.
- D4. My PRs that are neither "needs action" nor "ready to merge" (checks
  pending, waiting, drafts) go in "My PRs in progress", collapsed, after
  "ready to merge".
- D5. "Assigned / mentions" = open items assigned to me + unread
  notifications with reason mention / team_mention / assign. A row leaves
  when opened or done.
- D6. Home rows use the queue + reading pane (select, double-click / Enter
  opens a tab, j/k), so Approve / Merge / Reply work in place.
- D7. First run: until the first deep sync ends the footer shows "Syncing
  repos: N of M"; empty lists say "Still syncing…".
- D8. One-time Home note after auto-pin: "Pinned N repos you pushed to in the
  last 45 days. Change…" (opens Repositories), closable. Auto-pin rule
  unchanged.
- D9. Filters: `author:@me`, `assignee:@me`, `review-requested:@me` resolve to
  the viewer; `author:me` is accepted too; the placeholder shows `@me`.
- D10. Inbox: header action "Done: merged / closed" (one action, Undo in the
  footer) and "Done all" per group header. No automatic done.
- D11. Inbox marks a row read after it stays selected 1.5 s (not at once).
- D12. Batch merge of green bot PRs: agreed, later step (not step 5).
- D13. PR rows (Home and Pull requests) show one reason from cached data in
  place of the bare unread dot: checks failed, approved, changes requested,
  N new comments, new commits. "New since last view" line in the item view:
  later.
- D14. Sidebar count tooltip ("open PRs · open issues"); Explore 403 once per
  launch as a grey note; empty Review requests says "No review requests. PRs
  on your repos are on Home."
- D15. Step 5 builds D1-D11, D13, D14. Ships only with clippy, tests and the
  bench passing on AC power.

## Built (step 5)

- Home is a queue (`QueueKind::Home`, `fl_db::queries::queue::home_rank`)
  with six groups: Needs action, Waiting on me, Assigned and mentions, Ready
  to merge, My PRs in progress (folded), Recently updated (folded). Header
  click folds a group; j / k skip folded rows. Same reading pane as the lists.
  Home rows: open items of pinned repos or search hits (as Pull requests /
  Issues). "Mine" = author is the viewer; "my repo" = owner is the viewer or
  `repos.member`.
- Launch: `Workspace::new` goes to Home unless setting `open_home` is
  `false` (Settings view switch; palette "Open Home on launch: turn off/on").
  Bench runs keep the default layout. UI tests seed `open_home = false`.
- Sidebar and rail: Home first. Palette: "Go to: Home".
- Footer: "Syncing repos: N of M" while a pinned repo has no `repo:<id>`
  stamp (`fl_db::queries::sync::deep_progress`); empty lists say "Still
  syncing…". The Explore error is a muted note, once per launch
  (`Runtime::noted`), not a sync error.
- Auto-pin writes setting `auto_pin_note` (count); Home shows the note with
  "Change…" (opens Repositories) and × (deletes the row).
- Filters: `author:@me`, `assignee:@me`, `review-requested:@me` (and `me`).
- Inbox: "Done: merged / closed" in the header, "Done all" per group. Rows
  hide at once; an "N marked done · Undo" bar shows for 6 s, then the
  actions go out. Undo is in the Inbox, not the footer (the queue owns the
  pending list; closing the tab inside the 6 s also drops it).
- Inbox marks a row read after 1.5 s selected (`list_view::READ_AFTER`).
- List rows: one reason in place of the unread dot (`RowReason`: checks
  failed, changes requested, approved, N new comments, new commits; unread
  rows only) and a "bot" tag for GitHub App authors (avatar URL `/in/`).
- Sidebar pinned rows: tooltip "owner/name, N open PRs · N open issues".
- Empty Review requests: "No review requests. PRs on your repos are on Home."
