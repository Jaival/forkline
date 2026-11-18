# GPUI parity check (G1 step 7)

Date: 2026-11-18. Compared the Tauri app (`ui/`, `app/`) with the GPUI app
(`crates/fl-app`) after step 7. Source of truth: the Tauri feature folders
(`ui/src/features/*`, `ui/src/shell/*`) and the `fl_types::Action` kinds each
app sends.

## Views

Every Tauri view now has a GPUI view: Home, Inbox, Pulls, Issues, Reviews,
Issue, PR (conversation, commits, checks, files), Repo overview,
Repositories, Starred, Search. GPUI adds Explore, the reading pane, the
`Ctrl+K` palette, the `/` query bar with saved views, Done / Snooze and
Viewed files. Settings is a placeholder in both apps.

## What the Tauri app has and GPUI lacks

Writes (actions the Tauri UI sends, GPUI does not):

1. ~~**Issue state**~~: done 2026-11-18 (button and `X`).
2. **Labels and assignees**: edit pickers on the issue view
   (`SetLabels`, `SetAssignees`), with a pending marker per field.
3. ~~**PR merge**~~: done 2026-11-18 (menu, then confirm).
4. **Review events**: Request changes and Comment with a body. GPUI sends
   Approve only (`A`).
5. **Inline review comments**: select diff lines (Shift+click range), add
   drafts, send them with the review (`SubmitReview.comments`).
6. **Threads**: reply to a review thread (`ReplyThread`), resolve /
   unresolve (`ResolveThread`). GPUI shows threads read-only in the Files rail.
7. ~~**Notification done on GitHub**~~: done 2026-11-18; `E` also sends
   `NotificationDone`.

Shell:

8. ~~**Sign-in dialog**~~: done 2026-11-18 (`crates/fl-app/src/signin.rs`):
   device flow, plus PAT paste (the Tauri dialog has no paste). Reopens when
   GitHub rejects the token.
9. ~~**Failed actions**~~: done 2026-11-18: footer failed list with Retry /
   Discard, account box with Sign out.
10. ~~**Keys**~~: done 2026-11-18 (`Ctrl+Tab` / `Ctrl+Shift+Tab`).
11. **Inbox toggles**: unread done 2026-11-18 (`is:unread`, Unread
    toggle in every queue). Grouped / flat not done: GPUI is always grouped.

Not gaps: list filters (GPUI `/` covers state, author, label, repo,
assignee), repo pin (GPUI has Pin and `p`), search (GPUI is paged with
snippets, same `fl_search::search`).

## Order for the rest (agreed 2026-11-18)

Grouped by the UI each needs, smallest first. Each step is one PR with its
UI tests, a bench run and a handoff note.

1. **Small writes and keys**: Close / Reopen (1) as action-bar buttons and a
   key; Merge (3) as a split button with method menu (open, non-draft PRs);
   `Ctrl+Tab` / `Ctrl+Shift+Tab` (10); Inbox unread toggle (11) as an
   `is:unread` queue-filter term plus a header toggle; NotificationDone (7)
   if `E` should also send it.
2. **Review composer** (4): `R` on a PR opens a box with Comment / Approve /
   Request changes and a body (`SubmitReview`). Step 4 holds the draft review
   that inline comments add to.
3. **Labels and assignees** (2): picker popovers (search, multi-select) on
   the reading header, with the pending marker from `pending_fields`.
4. **Threads** (6): reply and resolve / unresolve in the Files rail.
5. **Inline review comments** (5): select diff lines in the Files tab, add
   drafts to the step-2 review. Largest, and the gpui-component `Diff` may
   not expose line selection: check its API first and report options if it
   blocks.

Then Explore sync (plan in `docs/HANDOFF.md`), then the Tauri removal (ask
first).

Step 1 is done (2026-11-18). Decided for 7: send it (GitHub "done" also
returns on new activity, like the local `done_at`).

## Recommendation (step 7 check)

Do not remove the Tauri app yet. Items 1-6, 8 and 9 lose real function
(writes and sign-in). Items 7, 10 and 11 are small. Suggested order: sign-in
(8) and Retry / Discard (9), then the PR / issue writes (1-6), then remove
`app/` and `ui/` and rename `fl-app` to `forkline` (ADR 0001).
