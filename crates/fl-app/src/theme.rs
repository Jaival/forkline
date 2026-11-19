//! Forkline's look: every colour and the type scale. Views read tokens from
//! [`Palette`] (through [`ActivePalette::palette`]) and sizes from the
//! constants here; they do no colour maths of their own.
//!
//! [`apply`] picks the palette for a mode and writes it over the
//! gpui-component theme too, so kit widgets (inputs, menus, buttons, the dock)
//! match. Values: `docs/design/mockups/` ("Forkline after", "Theme options").

use std::sync::Arc;

use gpui_kit::component::highlighter::{HighlightTheme, ThemeStyle};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Global, Hsla, Pixels, Rgba, Window, hsla, px};

// Type scale (px). Every text style also sets its line height, so a font with
// taller metrics cannot change a fixed row height.
pub const TEXT_BODY: Pixels = px(13.5);
pub const TEXT_SECONDARY: Pixels = px(12.5);
pub const TEXT_SMALL: Pixels = px(12.);
pub const TEXT_LABEL: Pixels = px(11.5);
pub const TEXT_PANE_TITLE: Pixels = px(17.);
pub const TEXT_READING_TITLE: Pixels = px(21.);
pub const LINE_BODY: Pixels = px(20.);
pub const LINE_SECONDARY: Pixels = px(18.);
pub const LINE_LABEL: Pixels = px(16.);

// Layout.
pub const RAIL_WIDTH: f32 = 52.;
pub const RAIL_BUTTON: f32 = 34.;
pub const PANE_HEADER: f32 = 48.;
pub const GROUP_HEADER: f32 = 30.;
pub const ROW_HEIGHT: f32 = 54.;
pub const FOOTER_HEIGHT: f32 = 28.;
pub const RADIUS: Pixels = px(6.);
pub const RADIUS_LG: Pixels = px(10.);

/// One GitHub state: icon / text colour, pill fill, pill text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StateColor {
    pub fg: Hsla,
    pub fill: Hsla,
    pub on_fill: Hsla,
}

/// Syntax colours for diffs and code blocks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Syntax {
    pub keyword: Hsla,
    pub string: Hsla,
    pub comment: Hsla,
    pub function: Hsla,
    pub type_: Hsla,
    pub number: Hsla,
    pub property: Hsla,
    pub punctuation: Hsla,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub name: &'static str,
    pub dark: bool,

    // Surfaces.
    /// Panes: queue, reading pane, active tab.
    pub bg: Hsla,
    /// Chrome: title bar, rail, tab bar, footer.
    pub surface: Hsla,
    /// Chips, count badges, the rail's active button, menus.
    pub raised: Hsla,
    /// Buttons and inputs.
    pub control: Hsla,
    pub hover: Hsla,
    /// The selected row.
    pub selected: Hsla,
    pub border: Hsla,
    /// Borders of buttons and inputs.
    pub border_strong: Hsla,
    /// Behind a modal dialog.
    pub backdrop: Hsla,

    // Text.
    /// Unread titles, the active tab.
    pub text_strong: Hsla,
    pub text: Hsla,
    /// Comment and description bodies.
    pub text_body: Hsla,
    /// Titles of read rows.
    pub text_read: Hsla,
    pub text_muted: Hsla,
    /// Counts, key hints, line numbers. Not for text that must be read.
    pub text_faint: Hsla,

    // Accent.
    /// Unread dots, the active tab underline, badges.
    pub accent: Hsla,
    /// The accent as text or an icon.
    pub accent_text: Hsla,
    /// Primary button fill (deep enough for `on_accent` text).
    pub accent_fill: Hsla,
    pub accent_fill_hover: Hsla,
    pub on_accent: Hsla,
    pub star: Hsla,

    // GitHub meaning; hue fixed, lightness per theme.
    pub open: StateColor,
    pub merged: StateColor,
    pub closed: StateColor,
    pub pending: StateColor,
    /// Drafts, notifications without a state.
    pub neutral: StateColor,
    /// `+N` / `-N` line counts.
    pub added: Hsla,
    pub removed: Hsla,

    // Diff.
    pub diff_add_bg: Hsla,
    pub diff_add_fg: Hsla,
    pub diff_del_bg: Hsla,
    pub diff_del_fg: Hsla,
    pub diff_hunk_bg: Hsla,
    pub diff_gutter: Hsla,
    /// Lines that hold a review thread.
    pub diff_thread: Hsla,
    pub syntax: Syntax,

    pub scrollbar_thumb: Hsla,
    /// Avatar fill and initials: (saturation, lightness) for each.
    avatar: (f32, f32, f32, f32),
}

const fn hsl(h: f32, s: f32, l: f32) -> Hsla {
    hsla(h / 360., s / 100., l / 100., 1.)
}

const fn hsl_a(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    hsla(h / 360., s / 100., l / 100., a)
}

const fn state(fg: Hsla, fill: Hsla, on_fill: Hsla) -> StateColor {
    StateColor { fg, fill, on_fill }
}

/// Dark default: cool slate, indigo accent.
pub const MIDNIGHT: Palette = Palette {
    name: "midnight",
    dark: true,
    bg: hsl(225., 12., 10.),
    surface: hsl(225., 13., 7.5),
    raised: hsl(225., 10., 15.),
    control: hsl(225., 10., 13.),
    hover: hsl(225., 10., 13.5),
    selected: hsl(228., 38., 17.5),
    border: hsl(225., 9., 16.5),
    border_strong: hsl(225., 9., 20.),
    backdrop: hsl_a(225., 30., 3., 0.55),
    text_strong: hsl(220., 14., 95.),
    text: hsl(220., 14., 92.),
    text_body: hsl(220., 14., 84.),
    text_read: hsl(220., 14., 76.),
    text_muted: hsl(222., 8., 60.),
    text_faint: hsl(222., 8., 48.),
    accent: hsl(228., 92., 69.),
    accent_text: hsl(228., 92., 74.),
    accent_fill: hsl(228., 72., 57.),
    accent_fill_hover: hsl(228., 72., 52.),
    on_accent: hsl(0., 0., 100.),
    star: hsl(42., 95., 58.),
    open: state(hsl(142., 65., 58.), hsl(142., 55., 20.), hsl(142., 70., 72.)),
    merged: state(hsl(270., 85., 72.), hsl(270., 40., 24.), hsl(270., 90., 82.)),
    closed: state(hsl(0., 80., 68.), hsl(0., 45., 22.), hsl(0., 85., 80.)),
    pending: state(hsl(42., 95., 58.), hsl(42., 60., 20.), hsl(42., 95., 70.)),
    neutral: state(hsl(222., 8., 60.), hsl(225., 10., 18.), hsl(222., 8., 76.)),
    added: hsl(142., 65., 60.),
    removed: hsl(0., 80., 68.),
    diff_add_bg: hsl(142., 40., 14.),
    diff_add_fg: hsl(142., 60., 76.),
    diff_del_bg: hsl(0., 45., 16.),
    diff_del_fg: hsl(0., 70., 80.),
    diff_hunk_bg: hsl(228., 25., 14.),
    diff_gutter: hsl(222., 8., 52.),
    diff_thread: hsl_a(42., 95., 58., 0.12),
    syntax: Syntax {
        keyword: hsl(270., 75., 77.),
        string: hsl(95., 45., 68.),
        comment: hsl(222., 8., 52.),
        function: hsl(212., 85., 74.),
        type_: hsl(178., 45., 64.),
        number: hsl(25., 85., 70.),
        property: hsl(200., 50., 76.),
        punctuation: hsl(222., 8., 66.),
    },
    scrollbar_thumb: hsl_a(222., 8., 60., 0.35),
    avatar: (45., 24., 85., 82.),
};

/// Light default: white with the same indigo accent.
pub const DAYLIGHT: Palette = Palette {
    name: "daylight",
    dark: false,
    bg: hsl(0., 0., 100.),
    surface: hsl(225., 25., 97.),
    raised: hsl(225., 20., 92.5),
    control: hsl(0., 0., 100.),
    hover: hsl(225., 25., 95.),
    selected: hsl(228., 95., 95.),
    border: hsl(225., 14., 90.),
    border_strong: hsl(225., 14., 84.),
    backdrop: hsl_a(225., 25., 12., 0.35),
    text_strong: hsl(225., 30., 8.),
    text: hsl(225., 25., 12.),
    text_body: hsl(225., 15., 22.),
    text_read: hsl(225., 12., 30.),
    text_muted: hsl(222., 10., 42.),
    text_faint: hsl(222., 8., 56.),
    accent: hsl(228., 78., 56.),
    accent_text: hsl(228., 78., 50.),
    accent_fill: hsl(228., 78., 56.),
    accent_fill_hover: hsl(228., 78., 50.),
    on_accent: hsl(0., 0., 100.),
    star: hsl(38., 95., 46.),
    open: state(hsl(137., 66., 32.), hsl(137., 60., 92.), hsl(137., 70., 24.)),
    merged: state(hsl(262., 60., 52.), hsl(262., 80., 95.), hsl(262., 55., 40.)),
    closed: state(hsl(356., 72., 46.), hsl(356., 85., 95.), hsl(356., 70., 36.)),
    pending: state(hsl(40., 100., 30.), hsl(45., 95., 90.), hsl(36., 100., 26.)),
    neutral: state(hsl(222., 10., 42.), hsl(225., 20., 93.), hsl(222., 12., 32.)),
    added: hsl(137., 66., 32.),
    removed: hsl(356., 72., 46.),
    diff_add_bg: hsl(137., 60., 94.),
    diff_add_fg: hsl(137., 55., 20.),
    diff_del_bg: hsl(356., 85., 96.),
    diff_del_fg: hsl(356., 60., 32.),
    diff_hunk_bg: hsl(228., 60., 96.),
    diff_gutter: hsl(222., 8., 50.),
    diff_thread: hsl_a(42., 95., 50., 0.14),
    syntax: Syntax {
        keyword: hsl(270., 55., 45.),
        string: hsl(130., 55., 27.),
        comment: hsl(222., 8., 44.),
        function: hsl(212., 75., 40.),
        type_: hsl(180., 70., 26.),
        number: hsl(25., 85., 36.),
        property: hsl(200., 70., 32.),
        punctuation: hsl(222., 10., 38.),
    },
    scrollbar_thumb: hsl_a(222., 10., 42., 0.3),
    avatar: (60., 90., 50., 29.),
};

impl Global for Palette {}

pub trait ActivePalette {
    fn palette(&self) -> &Palette;
}

impl ActivePalette for App {
    fn palette(&self) -> &Palette {
        self.global::<Palette>()
    }
}

/// Hues an avatar can take, picked by a hash of the login.
const AVATAR_HUES: [f32; 8] = [228., 160., 20., 330., 45., 190., 270., 100.];

/// FNV-1a: stable across runs and platforms (std's hasher is not).
pub(crate) fn stable_hash(s: &str) -> u32 {
    s.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

impl Palette {
    pub fn for_mode(dark: bool) -> &'static Palette {
        if dark { &MIDNIGHT } else { &DAYLIGHT }
    }

    /// Avatar fill and initials colour for `login`.
    pub fn avatar(&self, login: &str) -> (Hsla, Hsla) {
        let hue = AVATAR_HUES[stable_hash(login) as usize % AVATAR_HUES.len()];
        let (bg_s, bg_l, fg_s, fg_l) = self.avatar;
        (hsl(hue, bg_s, bg_l), hsl(hue, fg_s, fg_l))
    }

    /// A GitHub label colour made readable here: (border, text). Keeps the
    /// label's hue; greys stay grey.
    pub fn label(&self, color: Hsla) -> (Hsla, Hsla) {
        let h = color.h * 360.;
        let grey = color.s < 0.12;
        if self.dark {
            let s = if grey { 6. } else { (color.s * 100.).clamp(40., 90.) };
            (hsl(h, s * 0.7, 38.), hsl(h, s, 78.))
        } else {
            let s = if grey { 8. } else { (color.s * 100.).clamp(40., 85.) };
            (hsl(h, s * 0.7, 72.), hsl(h, s, 25.))
        }
    }

    fn highlight_theme(&self) -> Arc<HighlightTheme> {
        let base = if self.dark { HighlightTheme::default_dark() } else { HighlightTheme::default_light() };
        let mut theme = (*base).clone();
        theme.name = self.name.to_owned();
        let s = &self.syntax;
        let style = |c: Hsla| Some(theme_style(c));
        let syn = &mut theme.style.syntax;
        syn.keyword = style(s.keyword);
        syn.preproc = style(s.keyword);
        syn.boolean = style(s.number);
        syn.number = style(s.number);
        syn.constant = style(s.number);
        syn.string = style(s.string);
        syn.string_escape = style(s.number);
        syn.string_special = style(s.string);
        syn.comment = style(s.comment);
        syn.comment_doc = style(s.comment);
        syn.function = style(s.function);
        syn.constructor = style(s.type_);
        syn.type_ = style(s.type_);
        syn.enum_ = style(s.type_);
        syn.tag = style(s.keyword);
        syn.attribute = style(s.property);
        syn.property = style(s.property);
        syn.label = style(s.property);
        syn.operator = style(s.punctuation);
        syn.punctuation = style(s.punctuation);
        syn.punctuation_bracket = style(s.punctuation);
        syn.punctuation_delimiter = style(s.punctuation);
        syn.variable = None;
        theme.style.editor_background = Some(self.bg);
        theme.style.editor_foreground = Some(self.text);
        theme.style.editor_line_number = Some(self.diff_gutter);
        Arc::new(theme)
    }

    /// Write this palette over gpui-component's theme.
    fn write_kit(&self, t: &mut Theme) {
        t.font_size = TEXT_BODY;
        t.mono_font_size = TEXT_SECONDARY;
        t.radius = RADIUS;
        t.radius_lg = px(8.);
        t.shadow = false;
        t.highlight_theme = self.highlight_theme();
        let c = &mut t.colors;
        c.background = self.bg;
        c.foreground = self.text;
        c.border = self.border;
        c.input = self.border_strong;
        c.ring = self.accent;
        c.caret = self.accent;
        c.selection = self.accent.opacity(0.3);
        c.muted = self.raised;
        c.muted_foreground = self.text_muted;
        // The kit's "accent" is the hover fill of menu and list items.
        c.accent = self.hover;
        c.accent_foreground = self.text;
        c.primary = self.accent_fill;
        c.primary_hover = self.accent_fill_hover;
        c.primary_active = self.accent_fill;
        c.primary_foreground = self.on_accent;
        c.secondary = self.control;
        c.secondary_hover = self.hover;
        c.secondary_active = self.raised;
        c.secondary_foreground = self.text;
        c.button = self.control;
        c.button_hover = self.hover;
        c.button_active = self.raised;
        c.button_foreground = self.text;
        c.button_primary = self.accent_fill;
        c.button_primary_hover = self.accent_fill_hover;
        c.button_primary_active = self.accent_fill;
        c.button_primary_foreground = self.on_accent;
        c.button_secondary = self.control;
        c.button_secondary_hover = self.hover;
        c.button_secondary_active = self.raised;
        c.button_secondary_foreground = self.text;
        c.popover = if self.dark { self.control } else { self.bg };
        c.popover_foreground = self.text;
        c.list = self.bg;
        c.list_even = self.bg;
        c.list_head = self.bg;
        c.list_hover = self.hover;
        c.list_active = self.selected;
        c.list_active_border = self.accent;
        c.table = self.bg;
        c.table_even = self.bg;
        c.table_head = self.bg;
        c.table_head_foreground = self.text_muted;
        c.table_hover = self.hover;
        c.table_active = self.selected;
        c.table_active_border = self.accent;
        c.table_row_border = self.border;
        c.sidebar = self.surface;
        c.sidebar_foreground = self.text;
        c.sidebar_border = self.border;
        c.sidebar_accent = self.raised;
        c.sidebar_accent_foreground = self.text;
        c.sidebar_primary = self.accent_fill;
        c.sidebar_primary_foreground = self.on_accent;
        c.tab_bar = self.surface;
        c.tab_bar_segmented = self.raised;
        c.tab = self.surface;
        c.tab_foreground = self.text_muted;
        c.tab_active = self.bg;
        c.tab_active_foreground = self.text_strong;
        c.title_bar = self.surface;
        c.title_bar_border = self.border;
        c.status_bar = self.surface;
        c.status_bar_border = self.border;
        c.scrollbar = hsla(0., 0., 0., 0.);
        c.scrollbar_thumb = self.scrollbar_thumb;
        c.scrollbar_thumb_hover = self.text_muted.opacity(0.55);
        c.link = self.accent_text;
        c.link_hover = self.accent_text;
        c.link_active = self.accent_text;
        c.success = self.open.fg;
        c.success_foreground = self.on_accent;
        c.danger = self.closed.fg;
        c.danger_foreground = self.on_accent;
        c.warning = self.pending.fg;
        c.info = self.accent_text;
        c.magenta = self.merged.fg;
        c.drag_border = self.accent;
        c.drop_target = self.accent.opacity(0.15);
        c.switch = self.border_strong;
        c.slider_bar = self.accent;
        c.slider_thumb = self.bg;
        c.progress_bar = self.accent;
        c.skeleton = self.raised;
        c.group_box = self.surface;
        c.group_box_foreground = self.text;
    }
}

fn theme_style(color: Hsla) -> ThemeStyle {
    let Rgba { r, g, b, .. } = color.to_rgb();
    let hex = format!("#{:02x}{:02x}{:02x}", (r * 255.).round() as u8, (g * 255.).round() as u8, (b * 255.).round() as u8);
    serde_json::from_value(serde_json::json!({ "color": hex })).expect("a ThemeStyle from a colour")
}

/// The saved `theme` setting: `"dark"`, `"light"`, or absent (follow the
/// system). Anything else follows the system.
pub fn mode_from_setting(value: Option<&str>) -> Option<ThemeMode> {
    match value {
        Some("\"dark\"") => Some(ThemeMode::Dark),
        Some("\"light\"") => Some(ThemeMode::Light),
        _ => None,
    }
}

/// Switch to `mode` (`None`: the system's), then write Forkline's palette
/// for it over the kit theme.
pub fn apply(mode: Option<ThemeMode>, window: Option<&mut Window>, cx: &mut App) {
    match mode {
        Some(mode) => Theme::change(mode, window, cx),
        None => Theme::sync_system_appearance(window, cx),
    }
    let palette = *Palette::for_mode(Theme::global(cx).is_dark());
    Theme::update(cx, |t| palette.write_kit(t));
    cx.set_global(palette);
}

/// Relative luminance (WCAG 2). Test-only until the accent's contrast pass (G2 step 4).
#[cfg(test)]
pub fn luminance(c: Hsla) -> f32 {
    let Rgba { r, g, b, .. } = c.to_rgb();
    let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// WCAG contrast ratio of two opaque colours.
#[cfg(test)]
pub fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests;
