//! The "Appearance…" dialog: the mode (System / Light / Dark), one colour
//! choice per mode (a built-in theme or the custom accent), and the accent
//! itself (12 presets, a hue slider, muted colours, tinted backgrounds); on
//! the Fonts tab the interface font, the code font, "Hide secrets in code"
//! and a live sample.
//!
//! Every change applies at once: the dialog writes the edited [`Choices`] and
//! [`Fonts`] globals and shows the mode being edited. `Esc` or "Cancel" puts
//! back the choices from when it opened; "Done" keeps them and saves
//! `colors_dark`, `colors_light`, `accent`, `theme`, `ui_font`, `mono_font`
//! and `mask_secrets`. Like the sign-in dialog it never closes itself: it
//! emits [`AppearanceEvent`] and the workspace closes it.

use fl_db::queries::layout;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Sizable as _, ThemeMode, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Entity, EventEmitter, FocusHandle, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Pixels, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window, actions, div, px,
};

use crate::fonts::{self, CODE_FONTS, FontInfo, Fonts, UI_FONTS};
use crate::store::Store;
use crate::theme::{self, Accent, ActivePalette as _, Choices, Palette, Pick, RADIUS, THEMES, contrast, TextScale as _};

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Colours,
    Fonts,
}

pub struct Appearance {
    /// The choices when the dialog opened; Cancel puts them back.
    original: Choices,
    original_fonts: Fonts,
    fonts: Fonts,
    tab: Tab,
    /// The Fonts tab shows the chosen fonts' licences instead of the sample.
    licences: bool,
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
        let fonts = cx.try_global::<Fonts>().copied().unwrap_or_default();
        Self {
            original: choices,
            original_fonts: fonts,
            fonts,
            tab: Tab::Colours,
            licences: false,
            choices,
            editing_dark: dark,
            hue,
            focus,
            _subscription: subscription,
        }
    }

    /// The edited choices, written to the globals and shown in the edited mode.
    fn preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.set_global(self.choices);
        cx.set_global(self.fonts);
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

    fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        if tab == Tab::Fonts {
            // The pickers show every font in itself.
            fonts::load_all(cx);
        }
        self.tab = tab;
        cx.notify();
    }

    /// Open on the Fonts tab (the `--appearance-fonts` launch flag).
    pub fn show_fonts(&mut self, cx: &mut Context<Self>) {
        self.set_tab(Tab::Fonts, cx);
    }

    fn edit_fonts(&mut self, edit: impl FnOnce(&mut Fonts), window: &mut Window, cx: &mut Context<Self>) {
        edit(&mut self.fonts);
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
        cx.set_global(self.original_fonts);
        // The saved mode; in System mode that is the OS mode now, which may
        // have changed while the dialog was open.
        theme::apply(self.original.mode, Some(window), cx);
        window.refresh();
        cx.emit(AppearanceEvent::Close);
    }

    /// Keep the choices: show the saved mode and save all seven settings.
    fn done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.set_global(self.choices);
        cx.set_global(self.fonts);
        theme::apply(self.choices.mode, Some(window), cx);
        window.refresh();
        let mut rows = self.choices.settings().to_vec();
        rows.extend(self.fonts.settings().map(|(key, value)| (key, Some(value))));
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
                    .text_secondary()
                    .text_color(if selected { ui.text_strong } else { ui.text })
                    .when(selected, |d| d.font_weight(FontWeight::SEMIBOLD))
                    .truncate()
                    .child(p.title()),
            )
            .into_any_element()
    }
}

/// Font chips, each name drawn in its own font, in rows that wrap. Each chip
/// is as wide as its name (long names such as "Atkinson Hyperlegible Next"
/// are never cut off), and the chips in a row grow to fill it.
fn font_grid(
    id: &'static str,
    list: &'static [FontInfo],
    chosen: &'static FontInfo,
    ui: &Palette,
    cx: &mut Context<Appearance>,
    on_pick: fn(&mut Fonts, &'static FontInfo),
) -> impl IntoElement {
    h_flex().flex_wrap().gap(px(6.)).children(list.iter().enumerate().map(|(ix, f)| {
        let on = std::ptr::eq(f, chosen);
        div()
            .id((id, ix))
            .test_support()
            .flex_grow(1.)
            .h(px(30.))
            .px(px(10.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(RADIUS)
            .border_1()
            .border_color(if on { ui.accent_text } else { ui.border })
            .when(on, |d| d.bg(ui.selected).text_color(ui.text_strong))
            .when(!on, |d| d.text_color(ui.text).hover(|s| s.bg(ui.hover)))
            .cursor_pointer()
            .font_family(f.family)
            .text_body()
            // Monaspace Neon's line box puts its letters about 2 px below
            // the others'.
            .when(f.id == "monaspace-neon", |d| d.pb(px(2.)))
            .on_click(cx.listener(move |this, _, w, cx| this.edit_fonts(|fonts| on_pick(fonts, f), w, cx)))
            .child(div().whitespace_nowrap().child(f.title))
    }))
}

/// A queue row, a comment and a diff hunk in the chosen fonts.
fn sample(fonts: &Fonts, p: &Palette) -> impl IntoElement {
    let code = fonts.code().family;
    let row = h_flex()
        .h(px(crate::theme::ROW_HEIGHT))
        .px(px(12.))
        .gap(px(10.))
        .items_center()
        .rounded(RADIUS)
        .bg(p.selected)
        .child(div().size(px(7.)).flex_none().rounded_full().bg(p.accent))
        .child(
            v_flex()
                .min_w_0()
                .child(
                    div()
                        .text_body()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text_strong)
                        .truncate()
                        .child("Retry sync after a rate limit instead of failing"),
                )
                .child(
                    h_flex()
                        .gap(px(6.))
                        .text_small()
                        .text_color(p.text_muted)
                        .child(div().font_family(code).child("forkline/forkline"))
                        .child("#128 · jaival opened 2h ago"),
                ),
        );
    let comment = div()
        .text_body()
        .text_color(p.text_body)
        .child(
            h_flex()
                .flex_wrap()
                .child("Looks good. Can we read ")
                .child(div().px(px(4.)).rounded(px(4.)).bg(p.raised).font_family(code).child("retry_after"))
                .child(" from the response headers?"),
        );
    let line = |sign: &'static str, text: &'static str, bg: Hsla, fg: Hsla| {
        h_flex()
            .h(crate::theme::LINE_CODE)
            .px(px(8.))
            .bg(bg)
            .text_color(fg)
            .child(div().w(px(16.)).flex_none().child(sign))
            .child(div().whitespace_nowrap().child(text))
    };
    let hunk = v_flex()
        .overflow_hidden()
        .rounded(RADIUS)
        .border_1()
        .border_color(p.border)
        .font_family(code)
        .text_size(crate::theme::TEXT_CODE)
        .line_height(crate::theme::LINE_CODE)
        .child(line("", "@@ -12,3 +12,4 @@ fn client()", p.diff_hunk_bg, p.text_muted))
        .child(line(" ", "let base = config.api_url();", p.bg, p.text))
        .child(line("-", "let token = env::var(\"TOKEN\")?;", p.diff_del_bg, p.diff_del_fg))
        .child(line("+", "let token = \"ghp_x7Kq2mN9pR4tW8vY3zB6cD1fH5jL0sA2eG7i\";", p.diff_add_bg, p.diff_add_fg))
        .child(line("+", "let retry = Retry::after(Duration::from_secs(30));", p.diff_add_bg, p.diff_add_fg));
    v_flex()
        .id("appearance-sample")
        .test_support()
        .gap(px(10.))
        .p(px(10.))
        .rounded(px(8.))
        .bg(p.bg)
        .border_1()
        .border_color(p.border)
        .child(row)
        .child(comment)
        .child(hunk)
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
        .text_secondary()
        .cursor_pointer()
        .when(on, |d| d.bg(ui.selected).text_color(ui.text_strong).font_weight(FontWeight::SEMIBOLD))
        .when(!on, |d| d.text_color(ui.text_muted).hover(|s| s.bg(ui.hover)))
        .on_click(on_click)
        .child(label)
}

/// The hue slider's track: the swatch colour at every hue, left (0°) to
/// right (359°). GPUI gradients have two stops, so it is one segment per 30°.
/// `rem`: the window's rem size.
fn hue_strip(rem: Pixels) -> impl IntoElement {
    use gpui_kit::{linear_color_stop, linear_gradient};
    // The kit sizes the slider in rem (track 1.5 rem, bar 0.375 rem, thumb
    // 1 rem) but offsets the thumb by -5 px, so the thumb's centre is at
    // 1.0625 rem - 5 px from the top. Centre the 8 px strip there.
    let top = rem * 1.0625 - px(9.);
    h_flex().absolute().left_0().right_0().top(top).h(px(8.)).rounded_full().overflow_hidden().children((0..12u16).map(|i| {
        let (from, to) = (theme::swatch(i * 30), theme::swatch((i * 30 + 30).min(359)));
        div().flex_1().h_full().bg(linear_gradient(90., linear_color_stop(from, 0.), linear_color_stop(to, 1.)))
    }))
}

fn heading(text: impl Into<SharedString>, ui: &Palette) -> gpui_kit::Div {
    div().text_label().font_weight(FontWeight::SEMIBOLD).text_color(ui.text_muted).child(text.into())
}

impl Render for Appearance {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ui = *cx.palette();
        let rem = window.rem_size();
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
            cards.push(div().flex_1().min_w_0().p(px(3.)).border_2().border_color(ui.border.opacity(0.)).into_any_element());
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
                    .child(div().text_secondary().text_color(ui.text).child(format!("Any colour (hue {}°)", accent.hue)))
                    .child(
                        div()
                            .relative()
                            .child(hue_strip(rem))
                            .child(
                                Slider::new(&self.hue)
                                    .bg(gpui_kit::transparent_black())
                                    .text_color(ui.text_strong)
                                    .accessibility_label("Accent hue"),
                            ),
                    ),
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

        let tab = self.tab;
        let tabs = h_flex()
            .p(px(3.))
            .gap(px(2.))
            .rounded(RADIUS)
            .bg(ui.surface)
            .child(segment("appearance-tab-colours", "Colours", tab == Tab::Colours, &ui, cx.listener(|this, _, _, cx| this.set_tab(Tab::Colours, cx))))
            .child(segment("appearance-tab-fonts", "Fonts", tab == Tab::Fonts, &ui, cx.listener(|this, _, _, cx| this.set_tab(Tab::Fonts, cx))));

        let f = self.fonts;
        let fonts_tab = v_flex()
            .gap(px(16.))
            .child(
                v_flex()
                    .gap(px(6.))
                    .child(heading("Interface font", &ui))
                    .child(font_grid("appearance-ui-font", &UI_FONTS, f.ui, &ui, cx, |fonts, font| fonts.ui = font)),
            )
            .child(
                v_flex()
                    .gap(px(6.))
                    .child(heading("Code font", &ui))
                    .child(font_grid("appearance-code-font", &CODE_FONTS, f.mono, &ui, cx, |fonts, font| fonts.mono = font)),
            )
            .when(f.can_mask(), |d| {
                d.child(
                    Switch::new("appearance-mask")
                        .checked(f.mask_secrets)
                        .label("Hide secrets in code")
                        .on_click(cx.listener(|this, on: &bool, w, cx| this.edit_fonts(|fonts| fonts.mask_secrets = *on, w, cx))),
                )
            })
            .child(
                v_flex()
                    .gap(px(6.))
                    .child(heading(if self.licences { "Licences" } else { "Sample" }, &ui))
                    .map(|d| {
                        if self.licences {
                            let text = format!("{}\n\n{}", f.ui.licence, f.code().licence);
                            d.child(
                                div()
                                    .id("appearance-licences")
                                    .test_support()
                                    .h(px(186.))
                                    .overflow_y_scroll()
                                    .p(px(10.))
                                    .rounded(px(8.))
                                    .bg(ui.bg)
                                    .border_1()
                                    .border_color(ui.border)
                                    .text_small()
                                    .text_color(ui.text_muted)
                                    .child(text),
                            )
                        } else {
                            d.child(sample(&f, &ui))
                        }
                    }),
            );

        let footer_note = match tab {
            Tab::Colours => readout,
            Tab::Fonts => "All fonts: SIL Open Font License 1.1".to_owned(),
        };

        v_flex()
            .id("appearance")
            .key_context("Appearance")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::cancel))
            .w(px(680.))
            .max_h_full()
            .min_h_0()
            .p(px(20.))
            .gap(px(16.))
            .rounded(px(10.))
            .border_1()
            .border_color(ui.border)
            .bg(ui.control)
            .text_color(ui.text)
            .text_secondary()
            .shadow_lg()
            .child(
                v_flex()
                    .gap(px(2.))
                    .child(div().text_pane_title().font_weight(FontWeight::SEMIBOLD).text_color(ui.text_strong).child("Appearance"))
                    .child(div().text_color(ui.text_muted).child(match tab {
                        Tab::Colours => "One choice for dark mode and one for light. System follows Windows.",
                        Tab::Fonts => "The code font is for diffs, code in comments, commit SHAs and branch names.",
                    })),
            )
            .child(tabs)
            .child(
                // Scrolls when the window is too low for the whole dialog.
                v_flex()
                    .id("appearance-body")
                    .test_support()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap(px(16.))
                    .when(tab == Tab::Colours, |d| {
                        d.child(v_flex().gap(px(6.)).child(heading("Mode", &ui)).child(mode_row))
                            .child(v_flex().gap(px(6.)).child(heading("Colours for", &ui)).child(edit_row).child(grid))
                            .child(accent_section)
                    })
                    .when(tab == Tab::Fonts, |d| d.child(fonts_tab)),
            )
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(div().id("appearance-contrast").flex_1().text_color(ui.text_muted).child(footer_note))
                    .when(tab == Tab::Fonts, |d| {
                        d.child(
                            Button::new("appearance-licences-toggle")
                                .small()
                                .ghost()
                                .label(if self.licences { "Sample" } else { "Licences" })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.licences = !this.licences;
                                    cx.notify();
                                })),
                        )
                    })
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
