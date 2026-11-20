//! Forkline's look: every colour and the type scale. Views read tokens from
//! [`Palette`] (through [`ActivePalette::palette`]) and sizes from the
//! constants here; they do no colour maths of their own.
//!
//! [`apply`] picks the palette for a mode and writes it over the
//! gpui-component theme too, so kit widgets (inputs, menus, buttons, the dock)
//! match. Values: `docs/design/mockups/` ("Forkline after", "Theme options").

mod derive;

use std::sync::Arc;

use gpui_kit::component::highlighter::{HighlightTheme, ThemeStyle};
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Global, Hsla, Pixels, Rgba, Styled, Window, hsla, px};

pub use derive::{Accent, derive, swatch};

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
pub const LINE_SMALL: Pixels = px(16.);
pub const LINE_LABEL: Pixels = px(16.);
pub const LINE_PANE_TITLE: Pixels = px(24.);
pub const LINE_READING_TITLE: Pixels = px(28.);
/// Code in the diff: the kit's row height is 1.6 x the code font size.
pub const TEXT_CODE: Pixels = px(12.5);
pub const LINE_CODE: Pixels = px(20.);

/// Every (size, line height) pair the app sets; the font tests check that
/// each bundled font's letters fit inside these line boxes.
#[cfg(test)]
pub const TEXT_STYLES: [(Pixels, Pixels); 7] = [
    (TEXT_BODY, LINE_BODY),
    (TEXT_SECONDARY, LINE_SECONDARY),
    (TEXT_SMALL, LINE_SMALL),
    (TEXT_LABEL, LINE_LABEL),
    (TEXT_PANE_TITLE, LINE_PANE_TITLE),
    (TEXT_READING_TITLE, LINE_READING_TITLE),
    (TEXT_CODE, LINE_CODE),
];

/// The type scale as styles: each sets a size and its line height, so a font
/// with taller metrics cannot change a row's height.
pub trait TextScale: Styled + Sized {
    fn text_body(self) -> Self {
        self.text_size(TEXT_BODY).line_height(LINE_BODY)
    }
    fn text_secondary(self) -> Self {
        self.text_size(TEXT_SECONDARY).line_height(LINE_SECONDARY)
    }
    fn text_small(self) -> Self {
        self.text_size(TEXT_SMALL).line_height(LINE_SMALL)
    }
    fn text_label(self) -> Self {
        self.text_size(TEXT_LABEL).line_height(LINE_LABEL)
    }
    fn text_pane_title(self) -> Self {
        self.text_size(TEXT_PANE_TITLE).line_height(LINE_PANE_TITLE)
    }
    fn text_reading_title(self) -> Self {
        self.text_size(TEXT_READING_TITLE).line_height(LINE_READING_TITLE)
    }
}

impl<T: Styled> TextScale for T {}

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
    /// Repository dots: (saturation, lightness) of every [`REPO_HUES`] hue.
    repo: (f32, f32),
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
    repo: (70., 62.),
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
    repo: (75., 33.),
    avatar: (60., 90., 50., 29.),
};

/// Dark: neutral warm grey, amber accent.
pub const GRAPHITE: Palette = Palette {
    name: "graphite",
    dark: true,
    bg: hsl(30., 4., 10.),
    surface: hsl(30., 4., 7.5),
    raised: hsl(30., 4., 15.5),
    control: hsl(30., 4., 13.),
    hover: hsl(30., 4., 13.5),
    selected: hsl(36., 32., 16.),
    border: hsl(30., 4., 17.),
    border_strong: hsl(30., 4., 21.),
    backdrop: hsl_a(30., 10., 3., 0.55),
    text_strong: hsl(40., 10., 95.),
    text: hsl(40., 10., 91.),
    text_body: hsl(40., 8., 83.),
    text_read: hsl(40., 6., 75.),
    text_muted: hsl(35., 5., 61.),
    text_faint: hsl(35., 4., 48.),
    accent: hsl(36., 92., 60.),
    accent_text: hsl(36., 92., 64.),
    accent_fill: hsl(36., 92., 60.),
    accent_fill_hover: hsl(36., 92., 55.),
    on_accent: hsl(30., 40., 10.),
    star: hsl(48., 95., 60.),
    open: state(hsl(142., 60., 56.), hsl(142., 45., 19.), hsl(142., 65., 72.)),
    merged: state(hsl(270., 80., 74.), hsl(270., 35., 24.), hsl(270., 85., 84.)),
    closed: state(hsl(0., 78., 67.), hsl(0., 40., 22.), hsl(0., 85., 81.)),
    pending: state(hsl(45., 95., 58.), hsl(42., 55., 20.), hsl(45., 95., 72.)),
    neutral: state(hsl(35., 5., 61.), hsl(30., 4., 19.), hsl(35., 6., 78.)),
    added: hsl(142., 60., 58.),
    removed: hsl(0., 78., 67.),
    diff_add_bg: hsl(142., 35., 13.),
    diff_add_fg: hsl(142., 55., 76.),
    diff_del_bg: hsl(0., 40., 15.),
    diff_del_fg: hsl(0., 70., 80.),
    diff_hunk_bg: hsl(36., 20., 14.),
    diff_gutter: hsl(35., 5., 52.),
    diff_thread: hsl_a(48., 95., 60., 0.12),
    syntax: Syntax {
        keyword: hsl(15., 75., 70.),
        string: hsl(85., 45., 66.),
        comment: hsl(35., 5., 54.),
        function: hsl(42., 85., 68.),
        type_: hsl(175., 40., 62.),
        number: hsl(280., 55., 76.),
        property: hsl(200., 45., 74.),
        punctuation: hsl(35., 6., 66.),
    },
    scrollbar_thumb: hsl_a(35., 5., 61., 0.35),
    repo: (65., 64.),
    avatar: (40., 24., 80., 82.),
};

/// Dark, softer: lighter blue-grey, frosty cyan accent.
pub const FJORD: Palette = Palette {
    name: "fjord",
    dark: true,
    bg: hsl(218., 22., 15.),
    surface: hsl(220., 23., 12.),
    raised: hsl(218., 20., 20.5),
    control: hsl(218., 20., 18.),
    hover: hsl(218., 20., 18.5),
    selected: hsl(205., 35., 23.),
    border: hsl(218., 18., 22.),
    border_strong: hsl(218., 18., 27.),
    backdrop: hsl_a(220., 30., 5., 0.55),
    text_strong: hsl(215., 25., 94.),
    text: hsl(215., 25., 90.),
    text_body: hsl(215., 20., 84.),
    text_read: hsl(215., 16., 78.),
    text_muted: hsl(215., 14., 68.),
    text_faint: hsl(215., 10., 55.),
    accent: hsl(193., 60., 62.),
    accent_text: hsl(193., 65., 68.),
    accent_fill: hsl(193., 60., 62.),
    accent_fill_hover: hsl(193., 60., 56.),
    on_accent: hsl(218., 30., 12.),
    star: hsl(40., 85., 66.),
    open: state(hsl(142., 50., 62.), hsl(142., 35., 24.), hsl(142., 55., 78.)),
    merged: state(hsl(270., 70., 78.), hsl(270., 30., 29.), hsl(270., 80., 86.)),
    closed: state(hsl(0., 70., 72.), hsl(0., 35., 27.), hsl(0., 75., 84.)),
    pending: state(hsl(40., 85., 64.), hsl(40., 40., 24.), hsl(42., 90., 76.)),
    neutral: state(hsl(215., 14., 68.), hsl(218., 18., 24.), hsl(215., 16., 82.)),
    added: hsl(142., 50., 64.),
    removed: hsl(0., 70., 72.),
    diff_add_bg: hsl(150., 28., 19.),
    diff_add_fg: hsl(142., 50., 80.),
    diff_del_bg: hsl(0., 30., 21.),
    diff_del_fg: hsl(0., 65., 84.),
    diff_hunk_bg: hsl(205., 30., 20.),
    diff_gutter: hsl(215., 12., 58.),
    diff_thread: hsl_a(40., 85., 66., 0.12),
    syntax: Syntax {
        keyword: hsl(270., 60., 80.),
        string: hsl(95., 40., 72.),
        comment: hsl(215., 12., 62.),
        function: hsl(193., 60., 70.),
        type_: hsl(170., 40., 70.),
        number: hsl(25., 75., 74.),
        property: hsl(210., 55., 80.),
        punctuation: hsl(215., 14., 72.),
    },
    scrollbar_thumb: hsl_a(215., 14., 68., 0.35),
    repo: (60., 67.),
    avatar: (35., 28., 75., 84.),
};

/// Dark: deep green-black, emerald accent.
pub const FOREST: Palette = Palette {
    name: "forest",
    dark: true,
    bg: hsl(160., 14., 9.),
    surface: hsl(160., 16., 6.5),
    raised: hsl(160., 12., 14.5),
    control: hsl(160., 12., 12.),
    hover: hsl(160., 12., 12.5),
    selected: hsl(155., 35., 16.),
    border: hsl(160., 10., 16.),
    border_strong: hsl(160., 10., 20.),
    backdrop: hsl_a(160., 30., 3., 0.55),
    text_strong: hsl(150., 12., 95.),
    text: hsl(150., 12., 91.),
    text_body: hsl(150., 10., 83.),
    text_read: hsl(150., 8., 75.),
    text_muted: hsl(155., 7., 61.),
    text_faint: hsl(155., 6., 48.),
    accent: hsl(152., 60., 52.),
    accent_text: hsl(152., 60., 58.),
    accent_fill: hsl(152., 60., 52.),
    accent_fill_hover: hsl(152., 60., 46.),
    on_accent: hsl(160., 40., 8.),
    star: hsl(45., 90., 60.),
    open: state(hsl(130., 60., 60.), hsl(130., 45., 19.), hsl(130., 65., 76.)),
    merged: state(hsl(270., 80., 75.), hsl(270., 35., 24.), hsl(270., 85., 84.)),
    closed: state(hsl(0., 78., 68.), hsl(0., 40., 21.), hsl(0., 85., 81.)),
    pending: state(hsl(42., 92., 58.), hsl(42., 55., 19.), hsl(42., 95., 72.)),
    neutral: state(hsl(155., 7., 61.), hsl(160., 10., 18.), hsl(155., 8., 78.)),
    added: hsl(130., 60., 62.),
    removed: hsl(0., 78., 68.),
    diff_add_bg: hsl(140., 40., 13.),
    diff_add_fg: hsl(135., 55., 78.),
    diff_del_bg: hsl(0., 40., 15.),
    diff_del_fg: hsl(0., 70., 80.),
    diff_hunk_bg: hsl(160., 25., 13.),
    diff_gutter: hsl(155., 6., 52.),
    diff_thread: hsl_a(45., 90., 60., 0.12),
    syntax: Syntax {
        keyword: hsl(280., 60., 78.),
        string: hsl(75., 50., 66.),
        comment: hsl(155., 6., 54.),
        function: hsl(195., 70., 70.),
        type_: hsl(165., 50., 62.),
        number: hsl(30., 80., 68.),
        property: hsl(140., 35., 74.),
        punctuation: hsl(155., 7., 66.),
    },
    scrollbar_thumb: hsl_a(155., 7., 61., 0.35),
    repo: (65., 64.),
    avatar: (40., 22., 80., 82.),
};

/// Dark: plum-tinted, pink accent.
pub const ROSE: Palette = Palette {
    name: "rose",
    dark: true,
    bg: hsl(300., 10., 10.),
    surface: hsl(300., 11., 7.5),
    raised: hsl(300., 9., 15.5),
    control: hsl(300., 9., 13.),
    hover: hsl(300., 9., 13.5),
    selected: hsl(330., 35., 18.),
    border: hsl(300., 8., 17.),
    border_strong: hsl(300., 8., 21.),
    backdrop: hsl_a(300., 25., 3., 0.55),
    text_strong: hsl(320., 15., 95.),
    text: hsl(320., 15., 92.),
    text_body: hsl(320., 12., 84.),
    text_read: hsl(315., 9., 76.),
    text_muted: hsl(310., 7., 63.),
    text_faint: hsl(310., 6., 50.),
    accent: hsl(336., 85., 70.),
    accent_text: hsl(336., 85., 74.),
    accent_fill: hsl(336., 85., 70.),
    accent_fill_hover: hsl(336., 80., 64.),
    on_accent: hsl(336., 50., 12.),
    star: hsl(42., 95., 62.),
    open: state(hsl(142., 60., 58.), hsl(142., 45., 19.), hsl(142., 65., 74.)),
    merged: state(hsl(270., 85., 76.), hsl(270., 38., 25.), hsl(270., 90., 85.)),
    closed: state(hsl(5., 80., 66.), hsl(5., 42., 22.), hsl(5., 85., 80.)),
    pending: state(hsl(42., 95., 60.), hsl(42., 55., 20.), hsl(42., 95., 72.)),
    neutral: state(hsl(310., 7., 63.), hsl(300., 8., 19.), hsl(310., 8., 79.)),
    added: hsl(142., 60., 60.),
    removed: hsl(5., 80., 67.),
    diff_add_bg: hsl(142., 35., 13.),
    diff_add_fg: hsl(142., 55., 76.),
    diff_del_bg: hsl(355., 40., 16.),
    diff_del_fg: hsl(355., 70., 81.),
    diff_hunk_bg: hsl(330., 22., 15.),
    diff_gutter: hsl(310., 6., 54.),
    diff_thread: hsl_a(42., 95., 62., 0.12),
    syntax: Syntax {
        keyword: hsl(336., 75., 76.),
        string: hsl(85., 45., 68.),
        comment: hsl(310., 6., 55.),
        function: hsl(265., 75., 80.),
        type_: hsl(185., 45., 66.),
        number: hsl(25., 85., 72.),
        property: hsl(210., 55., 78.),
        punctuation: hsl(310., 8., 68.),
    },
    scrollbar_thumb: hsl_a(310., 7., 63., 0.35),
    repo: (65., 64.),
    avatar: (40., 24., 82., 83.),
};

/// Dark, accessible: pure black, white text, yellow accent, strong borders.
pub const HIGH_CONTRAST: Palette = Palette {
    name: "high-contrast",
    dark: true,
    bg: hsl(0., 0., 0.),
    surface: hsl(0., 0., 0.),
    raised: hsl(0., 0., 16.),
    control: hsl(0., 0., 8.),
    hover: hsl(0., 0., 14.),
    selected: hsl(55., 100., 16.),
    border: hsl(0., 0., 55.),
    border_strong: hsl(0., 0., 70.),
    backdrop: hsl_a(0., 0., 0., 0.7),
    text_strong: hsl(0., 0., 100.),
    text: hsl(0., 0., 100.),
    text_body: hsl(0., 0., 96.),
    text_read: hsl(0., 0., 92.),
    text_muted: hsl(0., 0., 82.),
    text_faint: hsl(0., 0., 70.),
    accent: hsl(55., 100., 55.),
    accent_text: hsl(55., 100., 60.),
    accent_fill: hsl(55., 100., 55.),
    accent_fill_hover: hsl(55., 100., 65.),
    on_accent: hsl(0., 0., 0.),
    star: hsl(55., 100., 55.),
    open: state(hsl(142., 75., 62.), hsl(142., 70., 16.), hsl(142., 85., 82.)),
    merged: state(hsl(270., 100., 80.), hsl(270., 55., 22.), hsl(270., 100., 88.)),
    closed: state(hsl(0., 100., 72.), hsl(0., 70., 20.), hsl(0., 100., 86.)),
    pending: state(hsl(40., 100., 62.), hsl(40., 80., 17.), hsl(40., 100., 78.)),
    neutral: state(hsl(0., 0., 82.), hsl(0., 0., 18.), hsl(0., 0., 92.)),
    added: hsl(142., 75., 62.),
    removed: hsl(0., 100., 72.),
    diff_add_bg: hsl(142., 60., 12.),
    diff_add_fg: hsl(142., 80., 82.),
    diff_del_bg: hsl(0., 60., 14.),
    diff_del_fg: hsl(0., 100., 87.),
    diff_hunk_bg: hsl(55., 60., 10.),
    diff_gutter: hsl(0., 0., 72.),
    diff_thread: hsl_a(55., 100., 55., 0.16),
    syntax: Syntax {
        keyword: hsl(275., 100., 82.),
        string: hsl(95., 70., 70.),
        comment: hsl(0., 0., 72.),
        function: hsl(205., 100., 75.),
        type_: hsl(175., 70., 62.),
        number: hsl(25., 100., 70.),
        property: hsl(195., 90., 80.),
        punctuation: hsl(0., 0., 88.),
    },
    scrollbar_thumb: hsl_a(0., 0., 82., 0.5),
    repo: (100., 78.),
    avatar: (50., 22., 90., 88.),
};

/// Light, warm: off-white, deep teal accent.
pub const PAPER: Palette = Palette {
    name: "paper",
    dark: false,
    bg: hsl(40., 30., 98.),
    surface: hsl(40., 22., 94.),
    raised: hsl(40., 16., 88.),
    control: hsl(40., 30., 99.),
    hover: hsl(40., 22., 95.),
    selected: hsl(178., 40., 90.),
    border: hsl(40., 14., 86.),
    border_strong: hsl(40., 14., 79.),
    backdrop: hsl_a(30., 20., 10., 0.35),
    text_strong: hsl(30., 18., 9.),
    text: hsl(30., 15., 14.),
    text_body: hsl(30., 12., 21.),
    text_read: hsl(30., 10., 28.),
    text_muted: hsl(30., 8., 35.),
    text_faint: hsl(30., 6., 52.),
    accent: hsl(178., 65., 30.),
    accent_text: hsl(178., 65., 26.),
    accent_fill: hsl(178., 65., 30.),
    accent_fill_hover: hsl(178., 65., 25.),
    on_accent: hsl(0., 0., 100.),
    star: hsl(36., 95., 42.),
    open: state(hsl(137., 66., 29.), hsl(137., 45., 88.), hsl(137., 70., 21.)),
    merged: state(hsl(262., 55., 48.), hsl(262., 60., 92.), hsl(262., 55., 36.)),
    closed: state(hsl(356., 72., 43.), hsl(356., 70., 92.), hsl(356., 70., 33.)),
    pending: state(hsl(36., 100., 28.), hsl(42., 85., 86.), hsl(34., 100., 23.)),
    neutral: state(hsl(30., 8., 35.), hsl(40., 16., 89.), hsl(30., 12., 26.)),
    added: hsl(137., 66., 29.),
    removed: hsl(356., 72., 43.),
    diff_add_bg: hsl(120., 40., 91.),
    diff_add_fg: hsl(137., 55., 18.),
    diff_del_bg: hsl(356., 65., 94.),
    diff_del_fg: hsl(356., 60., 30.),
    diff_hunk_bg: hsl(178., 30., 92.),
    diff_gutter: hsl(30., 6., 44.),
    diff_thread: hsl_a(42., 95., 50., 0.14),
    syntax: Syntax {
        keyword: hsl(320., 55., 38.),
        string: hsl(110., 50., 25.),
        comment: hsl(30., 8., 40.),
        function: hsl(210., 70., 36.),
        type_: hsl(178., 70., 23.),
        number: hsl(20., 85., 34.),
        property: hsl(195., 70., 29.),
        punctuation: hsl(30., 10., 34.),
    },
    scrollbar_thumb: hsl_a(30., 8., 35., 0.3),
    repo: (70., 33.),
    avatar: (55., 85., 50., 26.),
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

/// The dot colour of the repository `owner/name` in the active theme.
pub fn repo_color(full_name: &str, cx: &App) -> Hsla {
    let index = cx.try_global::<RepoColors>().map_or_else(|| RepoColors::default().index(full_name), |c| c.index(full_name));
    cx.palette().repo(index)
}

/// The colours a repository can take: hue and name. A repo's index comes
/// from a hash of `owner/name` unless the user picked one (`repos.color`).
pub const REPO_HUES: [(f32, &str); 10] = [
    (0., "Red"),
    (25., "Orange"),
    (48., "Yellow"),
    (90., "Lime"),
    (140., "Green"),
    (172., "Teal"),
    (198., "Cyan"),
    (220., "Blue"),
    (265., "Purple"),
    (315., "Pink"),
];

/// A repo hue keeps at least this far (degrees) from the theme's accent, so
/// a repo dot is not read as an unread dot.
const REPO_ACCENT_GAP: f32 = 15.;

/// A repo hue this close (degrees) to the accent also takes another tone:
/// lighter (dark themes) or darker (light themes) than the accent, and less
/// saturated next to a vivid accent. A hue shift alone left Midnight's Blue
/// repos looking like unread dots.
const REPO_ACCENT_NEAR: f32 = 25.;

/// Repository colours the user picked: `owner/name` -> index into
/// [`REPO_HUES`]. Loaded at startup; changed by [`RepoColors::set`].
#[derive(Clone, Debug, Default)]
pub struct RepoColors(std::collections::HashMap<String, u8>);

impl Global for RepoColors {}

impl RepoColors {
    pub fn new(picked: Vec<(String, u8)>) -> Self {
        Self(picked.into_iter().collect())
    }

    /// The repo's hue index: the picked one, else from the name's hash.
    pub fn index(&self, full_name: &str) -> u8 {
        self.picked(full_name).unwrap_or((stable_hash(full_name) % REPO_HUES.len() as u32) as u8)
    }

    pub fn picked(&self, full_name: &str) -> Option<u8> {
        self.0.get(full_name).copied()
    }

    pub fn set(&mut self, full_name: &str, index: Option<u8>) {
        match index {
            Some(i) => self.0.insert(full_name.to_owned(), i),
            None => self.0.remove(full_name),
        };
    }
}

/// Hues an avatar can take, picked by a hash of the login.
const AVATAR_HUES: [f32; 8] = [228., 160., 20., 330., 45., 190., 270., 100.];

/// FNV-1a: stable across runs and platforms (std's hasher is not).
pub(crate) fn stable_hash(s: &str) -> u32 {
    s.bytes().fold(0x811c_9dc5u32, |h, b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

/// Every built-in theme: the dark ones, then the light ones.
pub const THEMES: [&Palette; 8] = [&MIDNIGHT, &GRAPHITE, &FJORD, &FOREST, &ROSE, &HIGH_CONTRAST, &DAYLIGHT, &PAPER];

/// One mode's choice: a built-in theme (by `name`) or the custom accent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Theme(&'static str),
    Accent,
}

/// The appearance settings: the palette used in each mode (settings
/// `colors_dark` / `colors_light`: a built-in theme or the one derived from
/// `accent`), the accent, and the mode (`theme`; `None` = follow the system).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Choices {
    pub dark: Palette,
    pub light: Palette,
    pub accent: Accent,
    pub mode: Option<ThemeMode>,
}

impl Default for Choices {
    fn default() -> Self {
        Self { dark: MIDNIGHT, light: DAYLIGHT, accent: Accent::default(), mode: None }
    }
}

impl Global for Choices {}

impl Choices {
    /// From the saved `colors_dark` / `colors_light` values, for example
    /// `{"kind":"theme","name":"graphite"}` or `{"kind":"accent"}` (the
    /// palette derived from `accent`, see [`Accent::from_setting`]). Missing,
    /// malformed or unknown values, and a theme of the other mode, load as the
    /// default. `mode` stays `None`; see [`mode_from_setting`].
    pub fn from_settings(dark: Option<&str>, light: Option<&str>, accent: Option<&str>) -> Self {
        let accent = Accent::from_setting(accent);
        let pick = |value: Option<&str>, is_dark: bool, default: Palette| {
            let v: serde_json::Value = value.and_then(|v| serde_json::from_str(v).ok()).unwrap_or_default();
            match v["kind"].as_str() {
                Some("accent") => derive(accent, is_dark),
                Some("theme") => THEMES.into_iter().find(|p| p.dark == is_dark && v["name"] == p.name).copied().unwrap_or(default),
                _ => default,
            }
        };
        let d = Self::default();
        Self { dark: pick(dark, true, d.dark), light: pick(light, false, d.light), accent, mode: None }
    }

    pub fn palette(&self, dark: bool) -> &Palette {
        if dark { &self.dark } else { &self.light }
    }

    /// What the mode's palette is: a built-in theme or the accent.
    pub fn pick(&self, dark: bool) -> Pick {
        let name = self.palette(dark).name;
        THEMES.into_iter().find(|p| p.name == name).map_or(Pick::Accent, |p| Pick::Theme(p.name))
    }

    /// Use `pick` in the dark or light mode. An unknown or other-mode theme
    /// name changes nothing.
    pub fn set(&mut self, dark: bool, pick: Pick) {
        let palette = match pick {
            Pick::Accent => derive(self.accent, dark),
            Pick::Theme(name) => match THEMES.into_iter().find(|p| p.dark == dark && p.name == name) {
                Some(p) => *p,
                None => return,
            },
        };
        if dark { self.dark = palette } else { self.light = palette }
    }

    /// Change the accent; each mode that uses it gets the new palette.
    pub fn set_accent(&mut self, accent: Accent) {
        self.accent = accent;
        for dark in [true, false] {
            if self.pick(dark) == Pick::Accent {
                self.set(dark, Pick::Accent);
            }
        }
    }

    /// The settings rows these choices save as: (key, JSON value); `None`
    /// deletes the row (`theme` absent = follow the system).
    pub fn settings(&self) -> [(&'static str, Option<String>); 4] {
        let color = |dark: bool| match self.pick(dark) {
            Pick::Accent => r#"{"kind":"accent"}"#.to_owned(),
            Pick::Theme(name) => serde_json::json!({ "kind": "theme", "name": name }).to_string(),
        };
        let mode = self.mode.map(|m| if m.is_dark() { "\"dark\"" } else { "\"light\"" }.to_owned());
        [
            ("colors_dark", Some(color(true))),
            ("colors_light", Some(color(false))),
            ("accent", Some(self.accent.to_setting())),
            ("theme", mode),
        ]
    }
}

impl Palette {
    /// The name shown in the picker (`name` is the saved slug).
    pub fn title(&self) -> &'static str {
        match self.name {
            "midnight" => "Midnight",
            "graphite" => "Graphite",
            "fjord" => "Fjord",
            "forest" => "Forest",
            "rose" => "Rosé",
            "high-contrast" => "High contrast",
            "daylight" => "Daylight",
            "paper" => "Paper",
            _ => "Custom accent",
        }
    }

    /// Colour of repo hue `index` (see [`REPO_HUES`]) in this theme, moved
    /// away from the accent's hue when it is too close.
    pub fn repo(&self, index: u8) -> Hsla {
        let mut hue = REPO_HUES[index as usize % REPO_HUES.len()].0;
        let accent = self.accent.h * 360.;
        let d = (hue - accent + 540.) % 360. - 180.;
        if d.abs() < REPO_ACCENT_GAP {
            hue = (accent + REPO_ACCENT_GAP.copysign(d) + 360.) % 360.;
        }
        let (mut s, mut l) = self.repo;
        if d.abs() < REPO_ACCENT_NEAR {
            // Next to a vivid accent, less colour and a smaller step in
            // lightness keep the dot recognisably its hue.
            let step = if self.accent.s >= 0.6 {
                s *= 0.6;
                12.
            } else {
                20.
            };
            let accent_l = self.accent.l * 100.;
            l = if self.dark { l.max(accent_l + step).min(92.) } else { l.min(accent_l - step).max(15.) };
        }
        hsl(hue, s, l)
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
        // The kit's diff draws code at `rem * mono_font_size / 16` (rem = the
        // body size) in rows 1.6 times that: this gives TEXT_CODE / LINE_CODE.
        t.mono_font_size = TEXT_CODE * (16. / f32::from(TEXT_BODY));
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

/// Switch to `mode` (`None`: the system's), then write the palette chosen for
/// it ([`Choices`] global, else Midnight / Daylight) over the kit theme.
/// `mode` is what is shown; the saved mode is [`Choices::mode`].
pub fn apply(mode: Option<ThemeMode>, mut window: Option<&mut Window>, cx: &mut App) {
    match mode {
        Some(mode) => Theme::change(mode, window.as_deref_mut(), cx),
        None => Theme::sync_system_appearance(window.as_deref_mut(), cx),
    }
    let choices = cx.try_global::<Choices>().copied().unwrap_or_default();
    let palette = if Theme::global(cx).is_dark() { choices.dark } else { choices.light };
    let fonts = crate::fonts::register_chosen(cx);
    Theme::update(cx, |t| {
        palette.write_kit(t);
        crate::fonts::write_kit(&fonts, t);
    });
    cx.set_global(palette);
    if let Some(window) = window {
        sync_title_bar(window, palette.dark);
    }
}

/// Make the window's native title bar dark or light like the palette (GPUI
/// follows only the Windows app mode). No-op where the window has no Win32
/// handle (other platforms, test windows).
pub fn sync_title_bar(window: &Window, dark: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let RawWindowHandle::Win32(h) = handle.as_raw()
    {
        fl_win::set_dark_title_bar(h.hwnd.get(), dark);
    }
}

/// Relative luminance (WCAG 2).
pub fn luminance(c: Hsla) -> f32 {
    let Rgba { r, g, b, .. } = c.to_rgb();
    let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// WCAG contrast ratio of two opaque colours.
pub fn contrast(a: Hsla, b: Hsla) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
mod tests;
