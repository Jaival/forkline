//! Short transitions: hover, menus, dialogs, tab switch, sidebar collapse.
//!
//! Every one runs on GPUI's animation element or the kit's `transition`, so
//! with Windows' "Animation effects" off (read by `gpui_kit::init`, see
//! `docs/research/gpui-animation.md`) each shows its end state at once.
//! Tests run with reduced motion on (`test_support`); the tests here turn it
//! off.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::motion::{Presence, Transition, transition};
use gpui_kit::component::Selectable;
use gpui_kit::component::animation::{Lerp as _, ease_out_cubic};
use gpui_kit::component::button::Button;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::popover::Popover;
use gpui_kit::{
    Anchor, AnimationExt as _, AnyElement, App, Bounds, Global, Pixels, deferred, Context, DismissEvent, Div, ElementId, Entity, Focusable as _,
    Hsla, InteractiveElement, IntoElement, ParentElement, RenderOnce, SharedString, StatefulInteractiveElement,
    Styled, TestSupportExt as _, Window, div, px,
};

/// Hover tint fade in and out.
pub const HOVER: Duration = Duration::from_millis(90);
/// Menu and palette open.
pub const MENU: Duration = Duration::from_millis(120);
/// Dialog open.
pub const DIALOG: Duration = Duration::from_millis(160);
/// A tab's content fading in when it becomes the shown tab.
pub const TAB: Duration = Duration::from_millis(120);
/// Sidebar <-> rail width change.
pub const SIDEBAR: Duration = Duration::from_millis(160);

/// Fade `el` in while it slides down the last `travel` px into place. Plays
/// once each time `el` mounts under `id`.
pub fn enter<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    el: E,
    duration: Duration,
    travel: f32,
) -> impl IntoElement {
    el.with_animation(id, gpui_kit::Animation::new(duration).with_easing(ease_out_cubic), move |el, t| {
        // Opaque by mid-way: GPUI fades each primitive on its own, so a
        // half-faded surface shows its own shadow through it.
        el.opacity((t * 2.).min(1.)).top(px(-travel * (1. - t)))
    })
}

/// Fade a tab's content in (no slide: the tab bar stays still).
pub fn fade_in<E: IntoElement + Styled + 'static>(id: impl Into<ElementId>, el: E, duration: Duration) -> impl IntoElement {
    el.with_animation(id, gpui_kit::Animation::new(duration).with_easing(ease_out_cubic), |el, t| el.opacity(t))
}

/// Enter and exit progress (0..1) of something shown while `present`.
/// `None` once it has fully left: stop drawing it. Call it every frame,
/// shown or not; its state lives while it is called.
pub fn presence(id: impl Into<ElementId>, present: bool, duration: Duration, window: &mut Window, cx: &mut App) -> Option<f32> {
    let id: ElementId = id.into();
    let sample = Presence::new(id, present).transition(Transition::new(duration).ease(ease_out_cubic)).sample(window, cx);
    sample.should_render().then_some(sample.progress)
}

/// `el` at progress `t` of its enter (or, backwards, exit): faded, and
/// `travel` px above its place at 0.
pub fn surface<E: Styled>(el: E, t: f32, travel: f32) -> E {
    // Opaque from mid-way: GPUI fades each primitive on its own, so a
    // half-faded surface shows its own shadow through it.
    el.opacity((t * 2.).min(1.)).top(px(-travel * (1. - t)))
}

/// Over a closing copy: takes its clicks and wheel so it does nothing while
/// it fades out.
pub fn shield() -> Div {
    div().absolute().top_0().left_0().size_full().occlude()
}

/// An overlay (palette, dialog) kept on screen while it exits.
pub struct Exiting<T> {
    shown: Option<Entity<T>>,
}

impl<T> Default for Exiting<T> {
    fn default() -> Self {
        Self { shown: None }
    }
}

impl<T: 'static> Exiting<T> {
    /// What to draw this frame: the entity, its progress, and whether it is
    /// the live one (false: the closing copy). `live` is the open entity.
    pub fn sample(
        &mut self,
        id: &'static str,
        live: Option<&Entity<T>>,
        duration: Duration,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<(Entity<T>, f32, bool)> {
        let t = presence(id, live.is_some(), duration, window, cx);
        if let Some(live) = live {
            self.shown = Some(live.clone());
        }
        match t {
            Some(t) => self.shown.clone().map(|e| (e, t, live.is_some())),
            None => {
                self.shown = None;
                None
            }
        }
    }
}

/// A value that eases to `target` whenever it changes (the sidebar width).
pub fn ease_to(id: impl Into<ElementId>, target: f32, duration: Duration, window: &mut Window, cx: &mut App) -> f32 {
    let id: ElementId = id.into();
    transition(id, target, Transition::new(duration).ease(ease_out_cubic), window, cx)
}

/// An element whose background fades between `rest` and `hover`, in place
/// of an instant `.hover(|s| s.bg(..))`. `hover: None` (a selected row)
/// keeps `rest`.
#[derive(IntoElement)]
pub struct HoverBg<E: Hoverable> {
    inner: E,
    rest: Hsla,
    hover: Option<Hsla>,
}

/// What [`hover_bg`] wraps: a stateful div (in tests, behind
/// `test_support`).
pub trait Hoverable: StatefulInteractiveElement + Styled + ParentElement + IntoElement + 'static {}

impl<E: StatefulInteractiveElement + Styled + ParentElement + IntoElement + 'static> Hoverable for E {}

/// Give `inner` (which must have its own id) a fading hover background.
/// For a row on its parent's colour, `rest` is `hover` at zero alpha.
pub fn hover_bg<E: Hoverable>(inner: E, rest: Hsla, hover: Option<Hsla>) -> HoverBg<E> {
    HoverBg { inner, rest, hover }
}

impl<E: Hoverable> ParentElement for HoverBg<E> {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.inner.extend(elements);
    }
}

impl<E: Hoverable> RenderOnce for HoverBg<E> {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Tracked also while there is no hover colour, so a row that stops
        // being selected under the mouse shows its hover at once.
        let id = self.inner.interactivity().element_id.clone().expect("hover_bg needs an id");
        let hovered = window.use_keyed_state(ElementId::NamedChild(std::sync::Arc::new(id.clone()), "hovered".into()), cx, |_, _| false);
        let alpha = if *hovered.read(cx) && self.hover.is_some() { 1. } else { 0. };
        let alpha = transition(ElementId::NamedChild(std::sync::Arc::new(id), "hover-tint".into()), alpha, Transition::new(HOVER), window, cx);
        let hover = self.hover.unwrap_or(self.rest);
        self.inner.bg(self.rest.lerp(&hover, alpha)).on_hover(move |is, _, cx| {
            hovered.update(cx, |h, cx| {
                if *h != *is {
                    *h = *is;
                    cx.notify();
                }
            })
        })
    }
}

/// The kit's `DropdownMenu`, with the menu fading and sliding in as it
/// opens. Same call: `button.dropdown_menu(builder)`.
pub trait DropdownMenu: Selectable + InteractiveElement + IntoElement + 'static {
    fn dropdown_menu(
        mut self,
        builder: impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static,
    ) -> Menu<Self> {
        let id = self.interactivity().element_id.clone().unwrap_or(0.into());
        Menu { id: SharedString::from(format!("menu:{id:?}")), trigger: self, builder: Rc::new(builder) }
    }
}

impl DropdownMenu for Button {}

type Builder = Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

#[derive(IntoElement)]
pub struct Menu<T: Selectable + IntoElement + 'static> {
    id: SharedString,
    trigger: T,
    builder: Builder,
}

/// One dropdown's open menu and where it was last drawn.
#[derive(Default)]
struct MenuState {
    menu: Option<Entity<PopupMenu>>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl<T: Selectable + IntoElement + 'static> RenderOnce for Menu<T> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // `content` runs on every render, so build the menu once per open;
        // a closed menu is dropped and the next open rebuilds its items.
        let state = window.use_keyed_state(ElementId::from(self.id.clone()), cx, |_, _| MenuState::default());
        let builder = self.builder;
        let closed = state.downgrade();
        Popover::new(SharedString::from(format!("popover:{}", self.id)))
            .appearance(false)
            .overlay_closable(false)
            .trigger(self.trigger)
            .anchor(Anchor::TopLeft)
            .on_open_change(move |open, window, cx| {
                if *open {
                    // A reopen replaces the copy still fading out.
                    if cx.has_global::<ClosingMenu>() {
                        let closing = cx.global_mut::<ClosingMenu>();
                        closing.menu = None;
                    }
                    return;
                }
                // Every close (item, Escape, outside click, the trigger)
                // ends here: hand the menu to the exit copy.
                let Ok((menu, bounds)) = closed.update(cx, |s, _| (s.menu.take(), s.bounds.take())) else { return };
                if let (Some(menu), Some(bounds)) = (menu, bounds) {
                    start_menu_exit(menu, bounds, window, cx);
                }
            })
            .content(move |_, window, cx| {
                let menu = match state.read(cx).menu.clone() {
                    Some(menu) => menu,
                    None => {
                        let builder = builder.clone();
                        let menu = PopupMenu::build(window, cx, move |menu, window, cx| builder(menu, window, cx));
                        menu.focus_handle(cx).focus(window, cx);
                        // Weak: the menu owns this listener (as in the kit's
                        // DropdownMenu). Closing goes through
                        // `on_open_change`.
                        let popover = cx.entity().downgrade();
                        window
                            .subscribe(&menu, cx, move |_, _: &DismissEvent, window, cx| {
                                if let Some(popover) = popover.upgrade() {
                                    popover.update(cx, |p, cx| p.dismiss(window, cx));
                                }
                            })
                            .detach();
                        state.update(cx, |s, _| s.menu = Some(menu.clone()));
                        menu
                    }
                };
                let bounds = state.read(cx).bounds.clone();
                let id = ElementId::NamedInteger("menu-enter".into(), menu.entity_id().as_u64());
                enter(
                    id,
                    div()
                        // Where the menu landed (after the positioner flips
                        // it above its trigger), for the exit copy.
                        .on_children_prepainted(move |b, _, _| bounds.set(b.first().copied()))
                        .child(menu)
                        .id("menu-surface")
                        .test_support(),
                    MENU,
                    6.,
                )
            })
    }
}

/// The last closed menu, drawn by the workspace while it fades out.
#[derive(Default)]
pub struct ClosingMenu {
    menu: Option<(Entity<PopupMenu>, Bounds<Pixels>)>,
    generation: u64,
}

impl Global for ClosingMenu {}

pub(crate) fn start_menu_exit(menu: Entity<PopupMenu>, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    if cx.reduce_motion() {
        return;
    }
    let closing = cx.default_global::<ClosingMenu>();
    closing.generation += 1;
    closing.menu = Some((menu, bounds));
    let generation = closing.generation;
    window.refresh();
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(MENU).await;
            _ = cx.update(|window, cx| {
                let closing = cx.default_global::<ClosingMenu>();
                if closing.generation == generation {
                    closing.menu = None;
                    window.refresh();
                }
            });
        })
        .detach();
}

/// The closing menu's fade-out, if one is running. It takes no clicks.
pub fn closing_menu(cx: &App) -> Option<AnyElement> {
    let closing = cx.try_global::<ClosingMenu>()?;
    let (menu, bounds) = closing.menu.clone()?;
    let id = ElementId::NamedInteger("menu-exit".into(), closing.generation);
    let copy = div().id("menu-closing").test_support().w(bounds.size.width).child(menu).child(shield());
    let copy = copy.with_animation(id, gpui_kit::Animation::new(MENU).with_easing(ease_out_cubic), |el, t| surface(el, 1. - t, 6.));
    Some(
        deferred(div().absolute().left(bounds.origin.x).top(bounds.origin.y).child(copy))
            .with_priority(3)
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AnyWindowHandle, AppContext as _, Bounds, Pixels, TestAppContext};

    use super::*;
    use crate::test_support::open;
    use crate::theme::RAIL_WIDTH;

    fn bounds(cx: &mut TestAppContext, handle: AnyWindowHandle, id: &'static str) -> Bounds<Pixels> {
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.find(id).bounds()
        })
        .unwrap()
    }

    fn settle(cx: &mut TestAppContext, duration: Duration) {
        cx.executor().advance_clock(duration * 4);
        cx.run_until_parked();
    }

    /// Ctrl+B: the nav box narrows over a few frames and lands on the
    /// rail's width; reduced motion lands at once.
    #[gpui_kit::test]
    fn the_sidebar_width_eases_to_the_rail(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.update(|cx| cx.set_reduce_motion(false));
        cx.run_until_parked();
        let open_width = bounds(cx, handle, "nav").size.width;
        cx.update_window(handle, |_, window, cx| ws.update(cx, |ws, cx| ws.toggle_sidebar(window, cx))).unwrap();
        let first = bounds(cx, handle, "nav").size.width;
        assert!(first > px(RAIL_WIDTH) && first <= open_width, "first frame {first:?}");
        settle(cx, SIDEBAR);
        assert_eq!(bounds(cx, handle, "nav").size.width, px(RAIL_WIDTH));

        cx.update(|cx| cx.set_reduce_motion(true));
        cx.update_window(handle, |_, window, cx| ws.update(cx, |ws, cx| ws.toggle_sidebar(window, cx))).unwrap();
        assert_eq!(bounds(cx, handle, "nav").size.width, open_width);
    }

    /// The Appearance dialog slides down into place as it opens.
    #[gpui_kit::test]
    fn the_dialog_slides_in(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.update(|cx| cx.set_reduce_motion(false));
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| ws.update(cx, |ws, cx| ws.open_appearance(window, cx))).unwrap();
        let opening = bounds(cx, handle, "appearance-mode-system").origin.y;
        settle(cx, DIALOG);
        let settled = bounds(cx, handle, "appearance-mode-system").origin.y;
        assert!(opening < settled, "from {opening:?} to {settled:?}");
    }

    /// A menu slides in on each open, not only the first.
    #[gpui_kit::test]
    fn a_menu_slides_in_on_every_open(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, _) = open(cx, dir.path());
        cx.update(|cx| cx.set_reduce_motion(false));
        cx.run_until_parked();
        let open_menu = |cx: &mut TestAppContext| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click("nav-theme", cx);
            })
            .unwrap();
            bounds(cx, handle, "menu-surface").origin.y
        };
        let opening = open_menu(cx);
        settle(cx, MENU);
        let settled = bounds(cx, handle, "menu-surface").origin.y;
        assert!(opening < settled, "from {opening:?} to {settled:?}");

        cx.update_window(handle, |_, window, cx| {
            window.press("escape", cx);
            window.render_frame(cx);
        })
        .unwrap();
        // Let the exit copy finish: it covers the trigger while it fades.
        settle(cx, MENU);
        assert!(cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.try_find("menu-surface").is_none()
        })
        .unwrap());
        assert!(open_menu(cx) < settled, "reopening starts the motion over");
    }

    /// A closed dialog stays on screen while it fades out, but its copy
    /// takes no clicks: the mode does not change.
    #[gpui_kit::test]
    fn a_closing_dialog_fades_out_and_takes_no_clicks(cx: &mut TestAppContext) {
        use crate::theme::Choices;
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.update(|cx| cx.set_reduce_motion(false));
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| ws.update(cx, |ws, cx| ws.open_appearance(window, cx))).unwrap();
        settle(cx, DIALOG);
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.press("escape", cx);
            window.render_frame(cx);
        })
        .unwrap();
        cx.read(|cx| assert!(ws.read(cx).appearance().is_none()));
        let before = cx.read(|cx| cx.try_global::<Choices>().map(|c| c.mode));
        let shown = cx
            .update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                let shown = window.try_find("appearance-mode-dark").is_some();
                window.click("appearance-mode-dark", cx);
                shown
            })
            .unwrap();
        assert!(shown, "the closing copy is drawn");
        assert_eq!(cx.read(|cx| cx.try_global::<Choices>().map(|c| c.mode)), before);
        settle(cx, DIALOG);
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("appearance-mode-dark").is_none());
        })
        .unwrap();
    }

    /// Escape closes a menu at once (focus goes back), and a copy fades out
    /// where it was.
    #[gpui_kit::test]
    fn a_closed_menu_fades_out(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, _) = open(cx, dir.path());
        cx.update(|cx| cx.set_reduce_motion(false));
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("nav-theme", cx);
        })
        .unwrap();
        settle(cx, MENU);
        let open_at = bounds(cx, handle, "menu-surface");
        cx.update_window(handle, |_, window, cx| {
            window.press("escape", cx);
            window.render_frame(cx);
        })
        .unwrap();
        let copy = bounds(cx, handle, "menu-closing");
        assert_eq!(copy.origin, open_at.origin);
        settle(cx, MENU);
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("menu-closing").is_none());
            assert!(window.try_find("menu-surface").is_none());
        })
        .unwrap();
    }
}
