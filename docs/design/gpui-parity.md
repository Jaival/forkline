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

1. **Issue state**: Close / Reopen (`Action::SetState`).
2. **Labels and assignees**: edit pickers on the issue view
   (`SetLabels`, `SetAssignees`), with a pending marker per field.
3. **PR merge**: Merge commit / Squash / Rebase (`Merge`).
4. **Review events**: Request changes and Comment with a body. GPUI sends
   Approve only (`A`).
5. **Inline review comments**: select diff lines (Shift+click range), add
   drafts, send them with the review (`SubmitReview.comments`).
6. **Threads**: reply to a review thread (`ReplyThread`), resolve /
   unresolve (`ResolveThread`). GPUI shows threads read-only in the Files rail.
7. **Notification done on GitHub** (`NotificationDone`). GPUI's `E` is local
   only by design (step 4); decide if it should also send the action.

Shell:

8. **Sign-in dialog**: device flow and PAT paste (`features/auth`). GPUI has
   no sign-in UI; it uses `FORKLINE_GITHUB_TOKEN`, the saved token, or
   `gh auth token`. A user without these cannot sign in. On
   `Unauthorized` the Tauri app reopens the dialog.
9. **Failed actions**: footer Retry / Discard buttons
   (`action_retry`, `action_discard`). GPUI shows "N failed" only.
10. **Keys**: `Ctrl+Tab` / `Ctrl+Shift+Tab` cycle tabs.
11. **Inbox toggles**: "Unread only" and grouped / flat. GPUI has groups
    always and no unread filter (`/` has no `is:unread`).

Not gaps: list filters (GPUI `/` covers state, author, label, repo,
assignee), repo pin (GPUI has Pin and `p`), search (GPUI is paged with
snippets, same `fl_search::search`).

## Recommendation

Do not remove the Tauri app yet. Items 1-6, 8 and 9 lose real function
(writes and sign-in). Items 7, 10 and 11 are small. Suggested order: sign-in
(8) and Retry / Discard (9), then the PR / issue writes (1-6), then remove
`app/` and `ui/` and rename `fl-app` to `forkline` (ADR 0001).
