# GPUI animation (gpui-pre 0.3.11, gpui-kit 0.9.0)

Researched 2026-11-26 for G3 step 6 (animations). Sources: the crates in
`~/.cargo/registry/src/*/` (`gpui-pre-0.3.11`, `gpui-base-0.9.0`,
`gpui-component-0.9.0`) and `vendor/gpui-pre-windows`.

## What GPUI (gpui-pre) has

`gpui::AnimationExt` (`src/elements/animation.rs`), on every element:

- `el.with_animation(id, Animation::new(dur).with_easing(f), |el, t| ...)`:
  a one-shot (or `.repeat()`) timeline. `t` is the eased 0..1 value. State
  (start time) is element state keyed by `id`: it lives only while the
  element renders in consecutive frames. An element that unmounts and
  mounts again starts over; one that stays mounted with the same id stays
  finished. Put a generation in the id to replay.
- `with_animations(id, vec![..], |el, ix, t| ...)`: a chain.
- `with_spring(id, SpringAnimation::new(target), |el, v| ...)`: keeps
  position and velocity across target changes.
- `Animation::with_max_fps(n)`: throttles the re-render.
- Easing: `linear`, `quadratic`, `ease_in_out`, `ease_out_quint`, `bounce`,
  `pulsating_between`. gpui-kit adds `ease_out_cubic`, `ease_in_out_cubic`,
  `cubic_bezier` (`gpui_kit::component::animation`).
- While running, the element calls `window.request_animation_frame()`:
  the view redraws every frame until the timeline ends.

Limits:

- No element transforms: no scale or rotate of a subtree (only images and
  SVGs take a `TransformationMatrix`). Motion is opacity, position (`top`,
  `left`, margins) and size.
- `opacity` multiplies into each primitive; there is no group
  compositing. A translucent surface shows its own drop shadow through it
  (the kit ramps the shadow by `t^3` for this reason).
- Plain `.hover(|s| ...)` styles switch at once and cannot animate: GPUI
  applies them in paint, without a render.

## What gpui-base / gpui-component (the kit) add

- `gpui_kit::base::motion::transition(id, target, Transition::new(dur),
  window, cx)`: a CSS-like value transition. Returns the current value;
  when `target` changes it animates from the value at that instant (and
  reverses cleanly mid-flight). Works for any `Lerp` type: `f32`, `Pixels`,
  `Point<Pixels>`, `Hsla`, `Size`, `Bounds`. State is keyed element state.
- `motion::Presence` (`base::motion::presence`): keeps an element mounted
  during an exit transition.
- `Spring` (`base::motion`): spring policy for retargeted values.
- `theme.motion_tokens()`: `duration_fast` 120 ms, `duration_normal`
  180 ms, `duration_slow` 280 ms, `easing_enter` / `easing_exit` /
  `easing_move` cubic beziers.

Kit components that already animate:

- `Dialog` (kit): slide-down / fade entrance and exit. Forkline does not
  use the kit `Dialog`; its dialogs are its own `deferred` overlays.
- `TabBar` / `Tab` (`component::tab`): the selected indicator slides
  (200 ms). The PR reading tabs use it.
- `Select`, `Combobox`, `DatePicker` dropdowns: 150 ms fade + 8 px slide
  (`popover::dropdown_popup`, crate-private).
- Not animated: `DropdownMenu` / `PopupMenu` (the menus Forkline uses),
  `Popover`, the dock tabs, `Button` hover.

## Reduced motion

`App::reduce_motion()` makes every `with_animation`, `with_spring`,
`transition` and `Presence` show the end state at once and request no
frames. GPUI itself never reads the OS. gpui-base does
(`base/src/reduce_motion.rs`): `gpui_kit::init` (called in `main.rs`)
reads `SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION)` (Settings >
Accessibility > Visual effects > Animation effects) and follows it. Under
the test scheduler it is never read; tests call `cx.set_reduce_motion`.

So Forkline gets reduced motion for free as long as every animation goes
through these APIs (or checks `cx.reduce_motion()` itself).

## Cost model

An animation costs one full window redraw per display frame for its
duration (GPUI redraws the window on any dirty view; Forkline uses no
`cached` views). Short transitions (100-200 ms) cost a few dozen frames.
The per-frame cost is the same as any redraw of that screen, so the risk
is a screen that is slow to draw (the big diff), not the animation code.

## How Forkline uses it (G3 step 6)

See `crates/fl-app/src/motion.rs` and `docs/HANDOFF.md` "G3 step 6 state".
