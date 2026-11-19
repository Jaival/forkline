# Plan: look refresh, colour themes, custom accent and fonts (GPUI G2)

Date: 2026-11-17. Status: planned, not started.
Mockups: the "Forkline before / after", "Theme options" and "Custom accent"
boards on the design canvas (https://claude.ai/artifact/KB3eRiBLYpyWRS8SiJBpcA).

## When

Start after GPUI G1 is finished: steps 6 (palette and query bar) and 7
(remaining views, parity check, removal of the Tauri app). Nothing here
changes G1 work. G2 needs the Ctrl+K palette from step 6.

## Goal

1. Replace the stock `gpui-component` look with Forkline's own (the "after"
   board): own palette, a smaller type scale, rows without borders.
2. Let each user pick how the app looks: a **named theme** (eight built in)
   or a **custom accent**, one colour from which the rest is worked out.
3. Light theme parity, which `docs/design/ui-plan.md` left for later.
4. Let each user pick an interface font and a code font.
5. Give each repository its own colour, so a mixed Inbox shows at a glance
   which repo an item is from.

## Decisions

- **One `theme.rs` owns every colour.** Views read theme tokens only; no
  colour maths in views. Today views derive colours themselves (for example
  `theme.primary.opacity(0.12)` for the selected row in `queue.rs`, pill
  fills in `reading/header.rs`); move these to named tokens.
- **Default theme: Midnight** (slate, indigo accent). Built-in themes are the
  same eight as Tidemail: Midnight, Graphite, Fjord, Forest, Rosé, High
  contrast (dark); Daylight, Paper (light). Values: the canvas boards,
  copied without fonts to `docs/design/mockups/`.
- **System mode picks between two choices**: one dark choice, one light
  choice, each a theme or the custom accent. The rail's theme button becomes
  a small menu: System / Light / Dark, and "Appearance…".
- **Custom accent** = hue (0-359), chroma level (normal / muted), "tint
  backgrounds" (on / off). Per-mode lightness targets, then a contrast pass:
  text >= 7:1, muted text >= 4.5:1, accent as text or icon >= 3:1, button
  text on accent >= 4.5:1. High contrast uses fixed values.
- **GitHub meaning colours stay fixed in hue** (open green, merged purple,
  closed red, pending amber, diff added / removed). Their lightness and the
  diff tints are set per theme so they read on every background. They never
  take the accent's hue.
- **Label pills keep the label's GitHub colour**, adjusted for contrast per
  theme (GitHub's own colours are often too light or too dark).
- **Repository colours.** Each repo gets a colour, shown as an 8 px dot before
  the repo name on queue rows, on Issue / PR tabs, in the pinned-repos menu,
  the queue's repo filter and the repo header. By default the colour comes
  from a hash of `owner/name`, so it is stable and needs no setup. The user can
  pick another from the pinned-repos menu or the repo header. Colours come
  from a fixed set of 10 hues, tuned per theme to pass >= 3:1 on the
  background; they never use the accent's exact hue, so a repo dot is not
  mistaken for an unread dot.
- Same design and settings model as Tidemail
  (`tidemail/docs/design/themes-plan.md`), so the two apps match. They do not
  share code yet; a shared crate is an option once both have shipped it.

## Fonts

Each user picks an **interface font** (everything but code) and a **code font**
(diffs, inline code and code blocks in comments, commit SHAs, branch names).
Defaults: Inter and Obel Mono.

| Font | Used for | Why offer it |
|---|---|---|
| **Inter** (default) | interface | built for screens; very clear at 12-14 px |
| Geist | interface | tighter and more modern |
| IBM Plex Sans | interface | a little more character |
| Figtree | interface | friendly and round, still compact |
| Atkinson Hyperlegible Next | interface | made for low vision; pairs with High contrast |
| Source Serif 4 | interface (serif) | for reading-heavy use |
| Google Sans | interface | Google's own UI face; round and open |
| Space Grotesk | interface | quirky and technical; strongest in headings |
| Google Sans Flex | interface | adjustable Google Sans (width, roundness); could replace Google Sans |
| Manrope | interface | modern, geometric, clean |
| Plus Jakarta Sans | interface | modern with a little warmth |
| JetBrains Mono | code | tall letters, clear `0` / `O` and `1` / `l` / `I` |
| Geist Mono | code | matches Geist |
| IBM Plex Mono | code | matches IBM Plex Sans |
| Source Code Pro | code | light and classic |
| Atkinson Hyperlegible Mono | code | matches Atkinson; the most legible |
| Space Mono | code | retro and wide; pairs with Space Grotesk. Only regular and bold |
| Google Sans Code | code | pairs with Google Sans; Google uses it for code in Gemini |
| Commit Mono | code | neutral and calm, made for reading code |
| Monaspace Neon | code | GitHub's own code font family (Neon is its neutral member) |
| **Obel Mono** (default) | code | made for terminals, code and agents (Typographic Systems, 2026) |

All twenty-one are bundled and under the SIL Open Font License. Samples: the
"Font options" board on the canvas; the "after" boards use the defaults and
have a font switch in their Tweaks.

- **Hide secrets in code** (on by default). Obel Mono can show tokens and
  secrets as mask dots: a known key prefix (`ghp_`, `sk-ant-`, `AKIA` ...) keeps
  four characters and masks the rest, and a value after a name that says
  secret (`GITHUB_TOKEN=`, `"password": "`) keeps its first four characters.
  It is the font's `ss01` feature, so Forkline turns it on through the code
  text style's font features; no second font ships. It applies to diffs,
  code in comments and commit messages. It changes only what is drawn: copy,
  search and the clipboard still get the real text. The switch shows only
  while the code font is Obel Mono (other fonts have no such feature).
- **The diff uses the code font**; its line height stays fixed so the 10k-line
  scroll bench (`diff_scroll_fps`) measures the same work for every font.
- Same font list and settings model as Tidemail.
- **A fixed set, not any font on the PC.** Every font is tested against the
  fixed row heights and the contrast rules; an arbitrary installed font is
  not. The Windows system font is only the fallback.
- **The fonts are embedded in the exe**, with their licence texts. Only the two
  chosen fonts are registered at startup (`cx.text_system().add_fonts`); the
  others load when the picker previews them. Expect the exe to grow by about
  8-12 MB. The default font now loads on every launch, so measure cold launch
  before and after.
- **Fixed row heights stay fixed.** Virtual lists need one row height, so every
  text style sets an explicit line height. A font with taller metrics then
  cannot change a row height or clip text.
- **An unknown font name falls back to the default** (for example a
  hand-edited setting).
- **Text size is a separate, open question** (Small / Default / Large). Say if
  it should join this phase.

## Settings (`settings` table)

The existing `theme` key (`"dark"` / `"light"`; absent = system) stays. Add:

| Key | Value | Default |
|---|---|---|
| `colors_dark` | `{"kind":"theme","name":"midnight"}` or `{"kind":"accent"}` | Midnight |
| `colors_light` | same | Daylight |
| `accent` | `{"hue":268,"chroma":"normal","tint":true}` | indigo |
| `ui_font` | `"inter"` (a name from the Fonts table) | Inter |
| `mono_font` | `"obel-mono"` | Obel Mono |
| `mask_secrets` | `true` / `false` | `true` |

Unknown names or bad JSON load as the defaults, so launch never fails on them.

Repository colours live with the repo, not in `settings`: an additive
migration adds `repos.color` (a hue index 0-9, NULL = from the hash). It
survives a re-sync, because sync does not write that column.

## Steps

Each step: clippy `-D warnings` and `cargo test --workspace` pass, a note in
`docs/HANDOFF.md`, a PrintWindow check by eye in dark and light.

1. **Look refresh** (the "after" board).
   - `theme.rs` with the Midnight and Daylight palettes, written over the
     `gpui-component` theme after `Theme::change` (as in `main.rs` today).
   - Type scale: 13.5 px body, 12.5 px secondary, 11.5 px labels; pane title
     17 px; reading title 21 px. Also fix the open item "markdown body text
     renders at 16 px" here.
   - Rail 52 px with 34 px buttons, unread count badge instead of a dot.
   - Dock tabs: icon per view, rounded active tab; tab titles `#42 title`
     (open item in HANDOFF).
   - Queue: 48 px header, sentence-case group headings with no fill, 54 px
     rounded rows with no borders, author avatar (initials, colour from a hash
     of the login), unread accent dot, tinted selected fill (no white bar).
   - Reading pane: larger header, state pill with icon, `author wants to merge
     into base`, Approve as the one primary button, tab counts, conversation
     as avatars plus bordered cards.
   - Footer: sync icon and key hints on the right.
   - Custom title bar with the search field (`TitleBar`), if it fits the
     launch budget; else keep the native bar and note why.
   - Syntax colours in the diff (open item), with colours from the theme.
2. **All built-in themes.** The other six palettes. Diff and GitHub state
   tokens per theme. Light parity for every view, including the Files tab.
3. **Repository colours.** Migration (next free number), `fl_db::queries::repo::set_color`,
   the hue set per theme, the dot in queue rows, tabs, pinned menu, repo
   filter and repo header; "Colour" submenu in the pinned-repos menu and the
   repo header.
4. **Accent derivation.** `theme/derive.rs`, OKLCH, pure function. Token
   targets as on the canvas (dark: background L 0.19, surface 0.165, raised
   0.245, border 0.27, selected 0.29 at chroma 0.06, text 0.94, muted 0.71,
   accent 0.70 at chroma 0.16). Contrast pass.
5. **Picker.** "Appearance…" dialog from the rail menu and from Ctrl+K: theme
   cards with live preview, 12 accent swatches, hue slider, the two switches,
   contrast readout. Applies live; Esc restores.
6. **Fonts.** `fonts.rs`: the font table, embedded bytes, registration of the
   chosen fonts at startup, explicit line heights on every text style. A Fonts section in the Appearance dialog with
   a live sample (a queue row, a comment, a diff hunk), and the "Hide secrets
   in code" switch (`ss01` on the code text style). First check that GPUI on
   Windows applies `ss01` through DirectWrite; if it does not, ship Obel Mono
   Masked as a second font for the switch instead.
7. **Checks.** Screenshots of Inbox + reading pane + Files tab in each theme
   and three accents, and each bundled font once. Full `fl-bench gpui`: `launch_ms`, `app_startup_ms`
   and `switch_tab_ms` must stay in budget (theme apply happens before the
   first frame; measure `store_ready` -> `workspace_new` before and after).

## Tests

- `derive.rs`: every hue in steps of 5 degrees, both modes, both tint
  settings pass every contrast rule; so do all built-in themes.
- State and diff colours pass >= 3:1 on every background.
- Settings: missing, unknown and malformed values load as defaults; so do
  unknown font names.
- Each bundled font: queue rows, tabs and diff lines lay out at their fixed
  heights with no clipped text; `diff_scroll_fps` stays in budget with the
  widest code font.
- Hide secrets: with the switch on, a diff line holding `ghp_` + 36
  characters draws mask dots after the first four; copying the line gives
  the full token; with another code font the switch is hidden.
- Repository colours: the same repo gets the same colour on every run and
  after a re-sync; an override persists; every hue passes on every theme.
- UI test: switching theme re-renders an open queue and reading pane without
  losing the selection or scroll position.

## Out of scope

- Theme files, import / export, sharing.
- Text size (open question, see Fonts).
