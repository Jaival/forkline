//! Smooth mouse-wheel scrolling for a GPUI `list`.
//!
//! GPUI's `list` moves a whole wheel notch (three lines, 60 px) in one
//! frame, so the text jumps. [`SmoothWheel::element`] takes line-based wheel
//! events over the list before the list sees them and plays the distance
//! over a few frames with an ease-out. Pixel deltas (touchpads) and
//! horizontal scrolls still go to the list as before.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use gpui_kit::{
    DispatchPhase, HitboxBehavior, IntoElement, ListState, ScrollDelta, ScrollEvent, Styled as _, Window, canvas, px,
};

/// The distance of one wheel line: the line height `list` uses.
const LINE_PX: f32 = 20.;
/// Time constant of the ease-out: 95 % of a notch lands in about 3x this.
const EASE_MS: f32 = 35.;
/// Below this distance the rest lands at once.
const SNAP_PX: f32 = 0.5;

#[derive(Default)]
struct Animation {
    /// Distance still to scroll; positive is down.
    remaining: Cell<f32>,
    /// When the last step ran.
    last: Cell<Option<Instant>>,
}

#[derive(Clone, Default)]
pub struct SmoothWheel(Rc<Animation>);

impl SmoothWheel {
    /// Drop the distance not scrolled yet (the list shows another document).
    pub fn stop(&self) {
        self.0.remaining.set(0.);
        self.0.last.set(None);
    }

    /// Put this before the list, in the same parent: it covers the parent,
    /// so the parent must be `relative`.
    pub fn element(&self, list: &ListState) -> impl IntoElement {
        let (step, wheel) = (self.0.clone(), self.0.clone());
        let list = list.clone();
        canvas(
            move |bounds, window, _| {
                step.step(&list, window);
                window.insert_hitbox(bounds, HitboxBehavior::Normal)
            },
            move |_, hitbox, window, _| {
                window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                    // Capture: before the list's own (bubble) handler.
                    if phase != DispatchPhase::Capture || !hitbox.should_handle_scroll(window) {
                        return;
                    }
                    let ScrollDelta::Lines(lines) = event.delta else { return };
                    if lines.y == 0. || lines.x != 0. {
                        return;
                    }
                    // Wheel down gives a negative delta and scrolls down.
                    wheel.add(-lines.y * LINE_PX);
                    cx.stop_propagation();
                    window.refresh();
                });
            },
        )
        .absolute()
        .size_full()
    }
}

impl Animation {
    fn add(&self, distance: f32) {
        let rest = self.remaining.get();
        // A turn of the wheel the other way drops what is left.
        let rest = if rest * distance < 0. { 0. } else { rest };
        if rest == 0. {
            self.last.set(Some(Instant::now()));
        }
        self.remaining.set(rest + distance);
    }

    /// Scroll the part of the distance due by now; ask for another frame
    /// while some is left. Runs before the list lays out, in the same frame.
    fn step(&self, list: &ListState, window: &mut Window) {
        let rest = self.remaining.get();
        if rest == 0. {
            return;
        }
        let now = Instant::now();
        let ms = self.last.replace(Some(now)).map_or(0., |t| now.duration_since(t).as_secs_f32() * 1000.);
        let mut left = rest * (-ms / EASE_MS).exp();
        if left.abs() < SNAP_PX {
            left = 0.;
        }
        let top = -list.scroll_px_offset_for_scrollbar().y;
        let max = list.max_offset_for_scrollbar().y;
        let wanted = top + px(rest - left);
        let target = wanted.clamp(px(0.), max.max(px(0.)));
        list.scroll_by(target - top);
        // At either end the rest has nowhere to go.
        if target != wanted {
            left = 0.;
        }
        self.remaining.set(left);
        if left == 0. {
            self.last.set(None);
        } else {
            window.request_animation_frame();
        }
    }
}
