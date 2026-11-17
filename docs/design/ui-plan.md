# UI plan: Triage shell (GPUI G1)

Date: 2026-11-16. Mockups: `docs/design/ui-mockups.html` (B = shell, D = PR Files tab).

## Decisions

- Shell is direction B (Triage): icon rail, grouped queue list, reading pane.
- The PR Files tab uses direction D (Review desk): file list with Viewed
  state, diff in the centre, threads and checks in a right rail.
- Keep the dock, tabs, split panes and footer from G0. The reading pane is the
  default; `Enter` opens the selected item in its own tab.
- Launch view is Inbox, not Home. `ViewRef::Home` stays (saved layouts hold
  it); the default layout opens Inbox. The launch bench measures Inbox painted.
- Dark theme first. Light theme parity is a later pass; do not break it, do
  not polish it.
- Done and Snooze are in the first pass. They are local state only (GitHub has
  no snooze): one additive migration, `0009`, adding `done_at` and
  `snoozed_until` to the inbox item. An item returns to the queue when it
  updates after `done_at`, or when `snoozed_until` passes.
- Pinned repos leave the 240 px sidebar. They move to a rail flyout and to the
  queue's repo filter.
- Stack stays GPUI + gpui-component (ADR 0001). If a GPUI limit blocks a
  step, stop and report options.

## Steps

Status 2026-11-17: steps 1-6 done (see `docs/HANDOFF.md`, GPUI G1 state).

Each step: its `bench::ready` key and bench step, gpui_kit tests for its keys
(no synthetic keystrokes), clippy and tests pass, a note in `docs/HANDOFF.md`.

1. **Shell.** Replace the sidebar with an icon rail (Inbox, Pull requests,
   Issues, Reviews, Repositories, Starred, Explore, Search; settings and theme
   at the bottom). A list view is a queue pane plus a reading pane inside one
   dock panel. Default layout opens Inbox.
2. **Queue list.** One grouped, virtualized list component with `j` / `k` /
   `Enter`. Groups for Inbox: Review requested, Mentioned, Assigned, Your PRs.
   Reuse it for Pulls, Issues and Reviews. Rows: state icon, title, repo,
   number, author, checks, age, unread mark.
3. **Reading pane.** Issue and PR header, tabs (Conversation, Files, Commits,
   Checks), markdown comments through gpui-component `TextView`. Opening an
   item sets `last_viewed_at`. Budget: open cached item < 30 ms.
4. **Action bar and keys.** `A` approve, `R` reply, `E` done, `H` snooze,
   through the existing optimistic action queue. Migration 0009 lands here.
5. **PR Files tab.** D layout: file list, gpui-component `Diff`, thread rail,
   Viewed state. Budget: 10k-line diff scroll at 120 fps.
6. **Palette and query bar.** `Ctrl+K` commands, `/` filter with saved views,
   backed by `fl-search`.
7. **Remaining views.** Repo, Repositories, Starred, Explore, Search, Home.
   Then parity check and removal of the Tauri app (ask first).

## Open

- The `AGENTS.md` Checks section still lists the `bun` commands. For the GPUI
  work the checks are `cargo clippy --workspace --all-targets -- -D warnings`
  and `cargo test --workspace`; narrow: `cargo test -p fl-app`.
