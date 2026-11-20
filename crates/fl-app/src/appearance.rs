//! The "Appearance…" dialog: the mode (System / Light / Dark), one colour
//! choice per mode (a built-in theme or the custom accent), and the accent
//! itself (12 presets, a hue slider, muted colours, tinted backgrounds).
//!
//! Every change applies at once: the dialog writes the edited [`Choices`]
//! global and shows the mode being edited. `Esc` or "Cancel" puts back the
//! choices from when it opened; "Done" keeps them and saves `colors_dark`,
//! `colors_light`, `accent` and `theme`. Like the sign-in dialog it never
//! closes itself: it emits [`AppearanceEvent`] and the workspace closes it.

use fl_db::queries::layout;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Sizable as _, ThemeMode, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Entity, EventEmitter, FocusHandle, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window, actions, div, px,
};

use crate::store::Store;
use crate::theme::{
    self, Accent, ActivePalette as _, Choices, LINE_LABEL, Palette, Pick, RADIUS, TEXT_LABEL, TEXT_PANE_TITLE,
    TEXT_SECONDARY, THEMES, contrast,
};

#[cfg(test)]
mod tests;

actions!(appearance, [CancelAppearance]);

/// The board's presets: name and OKLCH hue.
pub const PRESETS: [(&str, u16); 12] = [
    ("Indigo", 268),
    ("Blue", 250),
    ("Sky", 230),
    ("Cyan", 210),
    ("Teal", 185),
    ("Green", 150),
    ("Lime", 125),
    ("Amber", 75),
    ("Orange", 50),
    ("Red", 25),
    ("Pink", 355),
    ("Violet", 300),
];

pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CancelAppearance, Some("Appearance"))]);
}

pub enum AppearanceEvent {
    Close,
}

impl EventEmitter<AppearanceEvent> for Appearance {}

pub struct Appearance {
    /// The choices when the dialog opened; Cancel puts them back.
    original: Choices,
    /// Dark or light as shown when it opened.
    original_dark: bool,
    choices: Choices,
    /// Which mode's choice is edited (and shown in the app meanwhile).
    editing_dark: bool,
    hue: Entity<SliderState>,
    focus: FocusHandle,
    _subscription: Subscription,
}

impl Appearance {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let choices = cx.try_global::<Choices>().copied().unwrap_or_default();
        let hue = cx.new(|_| SliderState::new().min(0.).max(359.).step(1.).default_value(choices.accent.hue as f32));
        let subscription = cx.subscribe_in(&hue, window, |this, _, ev: &SliderEvent, window, cx| {
            if let SliderEvent::Change(v) = ev {
                let hue = (v.start().round() as u16).min(359);
                this.edit_accent(Accent { hue, ..this.choices.accent }, window, cx);
            }
        });
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let dark = cx.palette().dark;
        Self { original: choices, original_dark: dark, choices, editing_dark: dark, hue, focus, _subscription: subscription }
    }

    /// The edited choices, written to the global and shown in the edited mode.
    fn preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.set_global(self.choices);
        let shown = if self.editing_dark { ThemeMode::Dark } else { ThemeMode::Light };
        theme::apply(Some(shown), Some(window), cx);
        window.refresh();
        cx.notify();
    }

    fn set_mode(&mut self, mode: Option<ThemeMode>, window: &mut Window, cx: &mut Context<Self>) {
        self.choices.mode = mode;
        self.editing_dark = mode.unwrap_or_else(|| window.appearance().into()).is_dark();
        self.preview(window, cx);
    }

    fn edit_mode(&mut self, dark: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.editing_dark = dark;
        self.preview(window, cx);
    }

    fn pick(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        self.choices.set(self.editing_dark, pick);
        self.preview(window, cx);
    }

    /// A new accent; the edited mode switches to the accent too.
    fn edit_accent(&mut self, accent: Accent, window: &mut Window, cx: &mut Context<Self>) {
        self.choices.set_accent(accent);
        self.choices.set(self.editing_dark, Pick::Accent);
        self.preview(window, cx);
    }

    fn cancel(&mut self, _: &CancelAppearance, window: &mut Window, cx: &mut Context<Self>) {
        cx.set_global(self.original);
        let shown = if self.original_dark { ThemeMode::Dark } else { ThemeMode::Light };
        theme::apply(Some(shown), Some(window), cx);
        window.refresh();
        cx.emit(AppearanceEvent::Close);
    }

    /// Keep the choices: show the saved mode and save all four settings.
    fn done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.set_global(self.choices);
        theme::apply(self.choices.mode, Some(window), cx);
        window.refresh();
        let rows = self.choices.settings();
        let db = Store::global(cx).read(cx).db().clone();
        cx.background_spawn(async move {
            let _ = db.write(|c| {
                for (key, value) in &rows {
                    match value {
                        Some(v) => layout::set_setting(c, key, v)?,
                        None => layout::delete_setting(c, key)?,
                    }
                }
                Ok(())
            });
        })
        .detach();
        cx.emit(AppearanceEvent::Close);
    }

    /// A theme card: a small picture of the app in `p`, then its name.
    fn card(&self, ix: usize, p: Palette, pick: Pick, cx: &mut Context<Self>) -> AnyElement {
        let ui = *cx.palette();
        let selected = self.choices.pick(self.editing_dark) == pick;
        let bar = |w: f32, c: Hsla| div().w(px(w)).h(px(5.)).rounded(px(2.)).bg(c);
        let row = |sel: bool, unread: bool| {
            h_flex()
                .h(px(16.))
                .px(px(5.))
                .gap(px(4.))
                .items_center()
                .rounded(px(3.))
                .when(sel, |d| d.bg(p.selected))
                .child(div().size(px(4.)).rounded_full().when(unread, |d| d.bg(p.accent)))
                .child(bar(if unread { 46. } else { 38. }, if unread { p.text } else { p.text_muted }))
        };
        let picture = h_flex()
            .h(px(64.))
            .overflow_hidden()
            .rounded(px(7.))
            .border_1()
            .border_color(p.border)
            .bg(p.bg)
            .child(
                v_flex()
                    .w(px(16.))
                    .h_full()
                    .items_center()
                    .pt(px(6.))
                    .gap(px(4.))
                    .bg(p.surface)
                    .child(div().size(px(8.)).rounded(px(2.)).bg(p.accent))
                    .child(div().size(px(6.)).rounded(px(2.)).bg(p.raised)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .p(px(5.))
                    .gap(px(2.))
                    .child(row(true, true))
                    .child(row(false, false))
                    .child(h_flex().mt(px(2.)).px(px(5.)).child(div().w(px(24.)).h(px(9.)).rounded(px(2.)).bg(p.accent_fill))),
            );
        v_flex()
            .id(("appearance-card", ix))
            .test_support()
            .flex_1()
            .min_w_0()
            .p(px(3.))
            .gap(px(5.))
            .rounded(px(10.))
            .border_2()
            .border_color(if selected { ui.accent_text } else { ui.surface.opacity(0.) })
            .cursor_pointer()
            .hover(|s| s.bg(ui.hover))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.pick(pick, window, cx)))
            .child(picture)
            .child(
                div()
                    .px(px(2.))
                    .text_size(TEXT_SECONDARY)
                    .line_height(px(18.))
                    .text_color(if selected { ui.text_strong } else { ui.text })
                    .when(selected, |d| d.font_weight(FontWeight::SEMIBOLD))
                    .truncate()
                    .child(p.title()),
            )
            .into_any_element()
    }
}

/// One button of a segmented control.
fn segment(
    id: &'static str,
    label: &'static str,
    on: bool,
    ui: &Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .flex_1()
        .h(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.))
        .text_size(TEXT_SECONDARY)
        .cursor_pointer()
        .when(on, |d| d.bg(ui.selected).text_color(ui.text_strong).font_weight(FontWeight::SEMIBOLD))
        .when(!on, |d| d.text_color(ui.text_muted).hover(|s| s.bg(ui.hover)))
        .on_click(on_click)
        .child(label)
}

fn heading(text: impl Into<SharedString>, ui: &Palette) -> gpui_kit::Div {
    div().text_size(TEXT_LABEL).line_height(LINE_LABEL).font_weight(FontWeight::SEMIBOLD).text_color(ui.text_muted).child(text.into())
}

impl Render for Appearance {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = *cx.palette();
        let dark = self.editing_dark;
        let accent = self.choices.accent;
        let accent_picked = self.choices.pick(dark) == Pick::Accent;

        let mode = self.choices.mode;
        let mode_row = h_flex()
            .p(px(3.))
            .gap(px(2.))
            .rounded(RADIUS)
            .bg(ui.surface)
            .child(segment("appearance-mode-system", "System", mode.is_none(), &ui, cx.listener(|this, _, w, cx| this.set_mode(None, w, cx))))
            .child(
                segment("appearance-mode-light", "Light", mode == Some(ThemeMode::Light), &ui, cx.listener(|this, _, w, cx| this.set_mode(Some(ThemeMode::Light), w, cx))),
            )
            .child(
                segment("appearance-mode-dark", "Dark", mode == Some(ThemeMode::Dark), &ui, cx.listener(|this, _, w, cx| this.set_mode(Some(ThemeMode::Dark), w, cx))),
            );
        let edit_row = h_flex()
            .p(px(3.))
            .gap(px(2.))
            .rounded(RADIUS)
            .bg(ui.surface)
            .child(segment("appearance-edit-dark", "Dark mode", dark, &ui, cx.listener(|this, _, w, cx| this.edit_mode(true, w, cx))))
            .child(segment("appearance-edit-light", "Light mode", !dark, &ui, cx.listener(|this, _, w, cx| this.edit_mode(false, w, cx))));

        // This mode's themes, then the accent, four to a row.
        let mut cards: Vec<AnyElement> = THEMES
            .into_iter()
            .filter(|p| p.dark == dark)
            .enumerate()
            .map(|(ix, p)| self.card(ix, *p, Pick::Theme(p.name), cx))
            .collect();
        let accent_ix = cards.len();
        cards.push(self.card(accent_ix, theme::derive(accent, dark), Pick::Accent, cx));
        while !cards.len().is_multiple_of(4) {
            cards.push(div().flex_1().min_w_0().into_any_element());
        }
        let mut grid = v_flex().gap(px(6.));
        let mut cards = cards.into_iter();
        loop {
            let row: Vec<AnyElement> = cards.by_ref().take(4).collect();
            if row.is_empty() {
                break;
            }
            grid = grid.child(h_flex().gap(px(6.)).children(row));
        }

        let swatches = h_flex().gap(px(6.)).children(PRESETS.iter().enumerate().map(|(ix, (name, hue))| {
            let on = accent_picked && accent.hue == *hue;
            let hue = *hue;
            div()
                .id(("appearance-swatch", ix))
                .test_support()
                .flex_1()
                .h(px(28.))
                .rounded(px(7.))
                .bg(theme::swatch(hue))
                .border_2()
                .border_color(if on { ui.text_strong } else { ui.surface.opacity(0.) })
                .cursor_pointer()
                .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(*name).build(window, cx))
                .on_click(cx.listener(move |this, _, window, cx| {
                    let accent = Accent { hue, ..this.choices.accent };
                    this.hue.update(cx, |s, cx| s.set_value(hue as f32, window, cx));
                    this.edit_accent(accent, window, cx);
                }))
        }));

        let edited = *self.choices.palette(dark);
        let ratio = |a: Hsla| format!("{:.1}:1", contrast(a, edited.bg));
        let readout = format!(
            "Text {} · muted text {} · accent {}",
            ratio(edited.text),
            ratio(edited.text_muted),
            ratio(edited.accent_text)
        );

        let accent_section = v_flex()
            .gap(px(10.))
            .child(heading("Custom accent", &ui))
            .child(swatches)
            .child(
                v_flex()
                    .gap(px(4.))
                    .child(div().text_size(TEXT_SECONDARY).text_color(ui.text).child(format!("Any colour (hue {}°)", accent.hue)))
                    .child(Slider::new(&self.hue).accessibility_label("Accent hue")),
            )
            .child(
                h_flex()
                    .gap(px(24.))
                    .child(
                        Switch::new("appearance-muted")
                            .checked(accent.muted)
                            .label("Muted colours")
                            .on_click(cx.listener(|this, on: &bool, w, cx| {
                                this.edit_accent(Accent { muted: *on, ..this.choices.accent }, w, cx)
                            })),
                    )
                    .child(
                        Switch::new("appearance-tint")
                            .checked(accent.tint)
                            .label("Tint backgrounds")
                            .on_click(cx.listener(|this, on: &bool, w, cx| {
                                this.edit_accent(Accent { tint: *on, ..this.choices.accent }, w, cx)
                            })),
                    ),
            );

        v_flex()
            .id("appearance")
            .key_context("Appearance")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::cancel))
            .w(px(680.))
            .p(px(20.))
            .gap(px(16.))
            .rounded(px(10.))
            .border_1()
            .border_color(ui.border)
            .bg(ui.control)
            .text_color(ui.text)
            .text_size(TEXT_SECONDARY)
            .shadow_lg()
            .child(
                v_flex()
                    .gap(px(2.))
                    .child(div().text_size(TEXT_PANE_TITLE).font_weight(FontWeight::SEMIBOLD).text_color(ui.text_strong).child("Appearance"))
                    .child(div().text_color(ui.text_muted).child("One choice for dark mode and one for light. System follows Windows.")),
            )
            .child(v_flex().gap(px(6.)).child(heading("Mode", &ui)).child(mode_row))
            .child(v_flex().gap(px(6.)).child(heading("Colours for", &ui)).child(edit_row).child(grid))
            .child(accent_section)
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(div().id("appearance-contrast").flex_1().text_color(ui.text_muted).child(readout))
                    .child(
                        Button::new("appearance-cancel")
                            .small()
                            .ghost()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, w, cx| this.cancel(&CancelAppearance, w, cx))),
                    )
                    .child(
                        Button::new("appearance-done")
                            .small()
                            .primary()
                            .label("Done")
                            .on_click(cx.listener(|this, _, w, cx| this.done(w, cx))),
                    ),
            )
    }
}
