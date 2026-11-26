//! Settings: app behaviour that is not appearance. One switch so far.

use fl_db::queries::layout;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Window, div, px,
};

use super::Built;
use crate::store::Store;
use crate::theme::{ActivePalette as _, TextScale as _};

/// Settings key: `false` keeps the last active tab at launch; anything else
/// (or no row) opens Home.
pub const OPEN_HOME_KEY: &str = "open_home";

/// Whether launch opens Home.
pub fn open_home(cx: &App) -> bool {
    Store::global(cx).read(cx).db().read(|c| layout::setting(c, OPEN_HOME_KEY)).ok().flatten().as_deref()
        != Some("false")
}

/// Save the launch choice.
pub fn set_open_home(on: bool, cx: &mut App) {
    let db = Store::global(cx).read(cx).db().clone();
    let value = if on { "true" } else { "false" };
    let _ = db.write(|c| layout::set_setting(c, OPEN_HOME_KEY, value));
    Store::emit_changed(&["settings"], cx);
}

pub struct SettingsView {
    open_home: bool,
    focus: FocusHandle,
}

impl SettingsView {
    pub fn build(cx: &mut App) -> Built {
        let on = open_home(cx);
        let view = cx.new(|cx| {
            cx.subscribe(&Store::global(cx), |this: &mut Self, _, ev: &fl_types::DataChanged, cx| {
                if ev.scopes.iter().any(|s| s == "settings") {
                    this.open_home = open_home(cx);
                    cx.notify();
                }
            })
            .detach();
            Self { open_home: on, focus: cx.focus_handle() }
        });
        let focus = view.read(cx).focus.clone();
        Built { view: view.into(), focus }
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        v_flex()
            .track_focus(&self.focus)
            .size_full()
            .p(px(24.))
            .gap(px(16.))
            .text_body()
            .child(div().text_pane_title().font_weight(FontWeight::SEMIBOLD).child("Settings"))
            .child(
                h_flex()
                    .max_w(px(560.))
                    .gap(px(16.))
                    .items_center()
                    .child(
                        v_flex()
                            .flex_1()
                            .gap(px(2.))
                            .child(div().text_color(p.text_strong).child("Open Home on launch"))
                            .child(
                                div()
                                    .text_small()
                                    .text_color(p.text_muted)
                                    .child("Off: the app opens on the tab that was active when it closed."),
                            ),
                    )
                    .child(Switch::new("settings-open-home").checked(self.open_home).on_click(cx.listener(
                        |this, on: &bool, _, cx| {
                            this.open_home = *on;
                            set_open_home(*on, cx);
                            cx.notify();
                        },
                    ))),
            )
    }
}
