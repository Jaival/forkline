# G5 plan: code reuse (2026-11-27)

Status: agreed with the user on 2026-11-27 (see Decisions). Starts after
G4 ships. Nothing is built.

Source: a read-only survey of the workspace on branch `g3-workflow` after
the rebase on `main` (2026-11-27). Line numbers below are from that survey
and will drift; re-check each one before its step starts.

## Size today

| Crate | Lines (.rs) |
|---|---|
| fl-app | 17,921 (views/ 3,607) |
| fl-sync | 4,096 |
| fl-db | 2,405 |
| fl-types | 956 |
| fl-search | 927 |
| fl-fake | 703 |
| fl-git | 569 |
| fl-win | 206 |
| fl-bench | 119 |
| **Total** | **27,902** |

Largest files: `fl-sync/src/sync.rs` 1,360, `fl-app/src/workspace.rs` 1,310,
`views/queue.rs` 1,164, `theme.rs` 1,088, `views/reading/files.rs` 936.

## What the payoff is

Removing duplicates saves only about 400 lines (~1.5 %). Line count is not
the goal. The goals are:

1. **One way to do each thing**, so a new view or sync call copies one
   helper, not one of four variants (empty states, time text, paging,
   store subscriptions).
2. **Fewer bugs from stringly typed data**: `DataChanged` scopes are string
   literals in 12 subscribe sites; a typo fails silently.
3. **Smaller big files**: `sync.rs`, `workspace.rs`, `queue.rs`.
4. **A widget crate (`fl-ui`)** with no Forkline data in it, so the
   widgets can be used in another GPUI app.

Each step must keep clippy, `cargo test --workspace` and every budget in
`bench/budgets.json` green. No visible change except where a step says so.

## Steps

Ordered by risk (low first) and by dependency.

### Step 1: small shared helpers (fl-app, low risk)

- One `empty_state(text, cx)` in `views/mod.rs`. Replaces `views/mod.rs`
  `empty()`, `views/repo.rs` `muted()`, `views/reading.rs` `message()`,
  `views/reading/tabs.rs` `message()`, and inline copies in
  `reading/files.rs` and `pickers.rs`.
- One generic `Load<T>` enum (today `views/repo.rs` and
  `reading/files.rs` each define one).
- One time-text function in the "2 min" style (decided): "12 s",
  "2 min", "3 h", "4 d", "3 wk", "2 y". Today `footer.rs` `ago()` gives
  this style up to days and `views/queue.rs` `age()` gives "5m", "2h",
  "4w", "2y". Visible change: list rows get the longer text; check the
  age column width in the queue. Tests in `footer.rs` and
  `queue/tests.rs` assert the strings.
- One `fmt_count(n)` for the "99+" badge (`rail.rs`, `sidebar.rs`).
- `rows()` / `rows_mut()` accessors in `explore.rs`, `repos.rs`,
  `search.rs`: a default method or a small macro.
- Ad-hoc `.hover(` (17 sites) to `motion::hover_bg` where the look is the
  same. Visual check with PrintWindow captures in two themes.

Saves ~100 lines. Bench: hover change only; run once.

### Step 2: fl-db query helpers (low risk)

- Callers of hand-written `limit + 1` / `take(limit)` paging
  (`explore.rs`, `pr.rs`, `repo.rs`, `lists.rs`, `inbox.rs`) use
  `queries/mod.rs` `page()`.
- One SQL fragment or fn for the `pending_fields` overlay (4 copies in
  `issue.rs`, `pr.rs`, `inbox.rs`). Shared row mappers (`issue_row` is
  used by `lists.rs` only; `inbox.rs` and `pr.rs` map inline).
- `tests/common/mod.rs` for `seeded()` / `fresh()` (5 test files).

Saves ~75 lines. Covered by fl-db tests; bench search + open item.

### Step 3: typed data-change scopes and store subscriptions (medium)

- Replace string scopes (`issues`, `repos`, `inbox`, `pr`, `sync`,
  `search`, ...) with an enum (bit set) in fl-types (decided), shared
  with fl-sync, which emits `sync`.
- One helper for the 12 `cx.subscribe(&Store::global(cx), ...)` sites,
  for example `store::on_change(cx, Scope::Issues | Scope::Inbox, |this, cx| ...)`.

Saves ~40 lines; removes the typo risk. Must land before step 4.

### Step 4: one `Rows` for every list (high risk)

- `views/queue.rs` re-implements `views/rows.rs` (`read_counts`,
  `load_more`, `reload`, generation, loading). Let `Rows` take a
  filter/snapshot fetch so the queue (Home, Inbox, PRs, Issues, Reviews)
  uses it.
- Depends on step 3. `queue/tests.rs` (657 lines) and the bench
  (`first_page_ms`, `open_home_ms`, switch tab) depend on it.
- Bench before and after, alternated with a step 3 build.

Saves ~100 lines. Splits `queue.rs` below 1,000 lines.

### Step 5: fl-sync paging helpers (medium)

- `rest_all` (REST `next_link` loops, 2 copies in `sync.rs`) and
  `graphql_pages` (cursor loops, 3 copies incl. `search_ids`) in
  `github.rs`, next to the generic `fill`.
- Split `sync.rs` (1,360 lines) by area: repos, deep sync, search lists,
  notifications.
- Covered by `fl-sync/tests/sync.rs` (wiremock). Check that rate-limit
  pauses and `MAX_EXTRA_PAGES` still apply per connection.

Saves ~40 lines.

### Step 6: `fl-ui` widget crate

In scope (decided): the widgets must work outside Forkline. Generic today:
`motion.rs` (enter, fade_in, presence, ease_to, hover_bg; not the menu
bits), `smooth_scroll.rs`, `toast.rs`, the palette / scale part of
`theme.rs`, `rail::avatar` and initials. Stays in fl-app: `views/rows.rs`,
`footer.rs`, `repo_color.rs`, `panel.rs`, `test_support.rs` (they use
Store, ViewRef or DB types).

Needs step 1 first. Cost: a theme trait boundary between fl-ui and the
app theme; gpui-kit stays pinned (`=0.9.0`) in both. Compile time may
improve a little (fl-app rebuilds less), not measured.

### Not in G5 (investigate first)

- fl-sync test seed vs fl-fake seeds: overlap unverified.
- fl-app test fixtures (`test_support.rs`): not fully audited.

## Decisions (2026-11-27)

1. Scope: steps 1-6, including the `fl-ui` crate.
2. Time text: the "2 min" style everywhere.
3. Scope enum: in fl-types.
4. One branch and PR per step.
5. Model: steps 1 and 2 are scoped work (Sonnet); steps 3-6 are
   refactors (Opus at high).

Multi-account is not in G4 or G5: it gets its own phase (schema change;
plan it before code).
