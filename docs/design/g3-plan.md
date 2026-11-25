# G3 plan: first real use

Source: the user's first pass on a real GitHub account (2026-11-25). Order
agreed with the user. Each step ends with clippy, `cargo test --workspace`,
and a bench run when it touches rendering.

## Facts from the real account

The real database (`%LOCALAPPDATA%/Forkline/forkline.db`) after the first
sync: 627 repos (503 own / member / starred, 124 explore-only, 426 starred),
132 notifications, **0 pinned repos, 0 issues**. Deep sync runs only for
pinned repos, so Pull requests, Issues, Reviews and the sidebar's Pinned list
are empty. The app looks broken on first sign-in.

## Steps

1. **Repos on first sign-in** (user item 2).
   - After the first sync, pin the user's own and org-member repos by
     activity (cap the number; starred repos are not pinned automatically).
   - Empty state with a "Pin repositories" step when nothing is pinned.
   - Pull requests / Issues / Reviews for all repos, not only pinned ones:
     GitHub search API (`is:pr author:@me`, `review-requested:@me`,
     `assignee:@me`) stored as list rows, deep sync still per pinned repo.
     Decide the storage with the user before coding (hard to undo).
2. **Quick fixes.**
   - Item 3: the account popup (user name -> Sign out) does not close.
     Close it on outside click and Esc.
   - Item 6: the palette has no "Sign out" when signed in. Add it ("Add
     account" later, with multi-account).
   - Item 8: tabs have no close button. Add a close button (hover + active).
   - Item 9: the filter field looks like the buttons next to it (border,
     own background), and the sidebar's selected row looks like its hover
     row. Make each one distinct, in all eight themes.
   - Item 5, Appearance dialog: the top of the dialog is cut off, "Done" has
     low contrast, the toggle knobs are dark on the accent, and the hue
     slider is not aligned (check thumb vs track and the gradient's hues
     vs the swatches).
3. **Font size** (item 7). Ctrl+= / Ctrl+- / Ctrl+0 and a size control in
   Appearance; saved setting; scales interface and code text.
4. **Custom title bar** (item 4), in parallel with step 5.
   - Rebuild it with the kit's `TitleBar::window_options` and measure.
     Before (handoff "Title bar: native"): `diff_scroll_fps` 240 -> 102.6 in
     every run, `switch_tab_ms` 4.1 -> 9.6, `open_cached_item_ms` 6 -> 15.
   - Find why. A steady 102.6 suggests a present-path cap, not more work
     per frame. First suspect: a transparent title bar makes the window
     use a premultiplied-alpha DirectComposition swap chain
     (`vendor/gpui-pre-windows`), which DWM composes on every present.
     Check with PIX / GPUView or timings around `Present`; try an opaque
     swap chain with a client-drawn caption (`WM_NCCALCSIZE` only, no
     transparency).
   - Ship only if the budgets in `bench/budgets.json` still pass, or the
     user accepts the cost.
   - The test `the_main_window_keeps_the_native_title_bar` guards the
     current state; change it with the new design.
5. **Workflow review** (item 10). List the main jobs ("what needs my
   review", "what changed on my PRs", "triage my repo's new issues",
   "follow a notification"). Walk each one in the app with real data and
   record the friction. Agree changes with the user (likely: a first-run
   flow, a Home that answers "what needs me now").
6. **Animations** (item 1). Short transitions for hover, menus, the
   dialog, tab switch and the sidebar collapse. Check first what GPUI
   offers (`with_animation`) and what each costs in the bench.

## Not in G3

- Multi-account ("Add account").
- OAuth App logo upload (GitHub's upload failed; retry later with
  `docs/design/logo/forkline-github-512.png`).
