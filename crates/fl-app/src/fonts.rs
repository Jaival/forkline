//! The bundled fonts: 11 interface fonts and 10 code fonts, all under the SIL
//! Open Font License, embedded in the exe with their licence texts
//! (`crates/fl-app/fonts/<id>/OFL.txt`). Only these are offered; the Windows
//! system font is only the fallback.
//!
//! Only the chosen fonts are registered at startup ([`apply`]); the others
//! are registered when the Appearance dialog previews them ([`load_all`]).
//! Settings: `ui_font` and `mono_font` (a font id as JSON, e.g. `"inter"`),
//! `mask_secrets` (`true` / `false`) and `font_size` (a percent of the
//! default size, one of [`SIZES`]). Unknown names and bad JSON load the
//! defaults.
//!
//! The font size scales the kit's rem (the body text size), so kit widgets,
//! the diff's code, and the app's text and text rows
//! ([`crate::theme::scaled`]) grow with it. Paddings, widths and icons stay.
//!
//! "Hide secrets in code" draws tokens and secrets as mask dots. It is Obel
//! Mono's `ss01` feature, which DirectWrite applies, but GPUI passes font
//! features down to every text run below an element, and the kit's diff and
//! markdown views give no hook to set them on the code text alone: interface
//! fonts below them would switch on their own `ss01` (Inter's alternate
//! digits, for one). So the switch selects Obel Mono Masked, the foundry's
//! build with `ss01` on by default, as the code font family.

use std::borrow::Cow;
use std::collections::HashSet;

use gpui_kit::component::Theme;
use gpui_kit::{App, Global};

/// One bundled font.
#[derive(Debug, PartialEq, Eq)]
pub struct FontInfo {
    /// The settings value, e.g. `"inter"`.
    pub id: &'static str,
    /// The family name the font files register.
    pub family: &'static str,
    pub title: &'static str,
    pub files: &'static [&'static [u8]],
    pub licence: &'static str,
}

macro_rules! font {
    ($id:literal, $family:literal, $title:literal, [$($file:literal),+ $(,)?]) => {
        FontInfo {
            id: $id,
            family: $family,
            title: $title,
            files: &[$(include_bytes!(concat!("../fonts/", $id, "/", $file))),+],
            licence: include_str!(concat!("../fonts/", $id, "/OFL.txt")),
        }
    };
}

pub const UI_FONTS: [FontInfo; 11] = [
    font!("inter", "Inter", "Inter", ["Inter[opsz,wght].ttf"]),
    font!("geist", "Geist", "Geist", ["Geist[wght].ttf"]),
    font!("ibm-plex-sans", "IBM Plex Sans", "IBM Plex Sans", ["IBMPlexSans[wdth,wght].ttf"]),
    font!("figtree", "Figtree", "Figtree", ["Figtree[wght].ttf"]),
    font!(
        "atkinson-hyperlegible-next",
        "Atkinson Hyperlegible Next",
        "Atkinson Hyperlegible Next",
        ["AtkinsonHyperlegibleNext[wght].ttf"]
    ),
    font!("source-serif-4", "Source Serif 4", "Source Serif 4", ["SourceSerif4[opsz,wght].ttf"]),
    font!("google-sans", "Google Sans", "Google Sans", ["GoogleSans[GRAD,opsz,wght].ttf"]),
    font!("space-grotesk", "Space Grotesk", "Space Grotesk", ["SpaceGrotesk[wght].ttf"]),
    font!("google-sans-flex", "Google Sans Flex", "Google Sans Flex", ["GoogleSansFlex[ROND,opsz,slnt,wdth,wght].ttf"]),
    font!("manrope", "Manrope", "Manrope", ["Manrope[wght].ttf"]),
    font!("plus-jakarta-sans", "Plus Jakarta Sans", "Plus Jakarta Sans", ["PlusJakartaSans[wght].ttf"]),
];

pub const CODE_FONTS: [FontInfo; 10] = [
    font!("obel-mono", "Obel Mono", "Obel Mono", ["ObelMono[wght].ttf"]),
    font!("jetbrains-mono", "JetBrains Mono", "JetBrains Mono", ["JetBrainsMono[wght].ttf"]),
    font!("geist-mono", "Geist Mono", "Geist Mono", ["GeistMono[wght].ttf"]),
    font!(
        "ibm-plex-mono",
        "IBM Plex Mono",
        "IBM Plex Mono",
        ["IBMPlexMono-Regular.ttf", "IBMPlexMono-Medium.ttf", "IBMPlexMono-SemiBold.ttf", "IBMPlexMono-Bold.ttf"]
    ),
    font!("source-code-pro", "Source Code Pro", "Source Code Pro", ["SourceCodePro[wght].ttf"]),
    font!(
        "atkinson-hyperlegible-mono",
        "Atkinson Hyperlegible Mono",
        "Atkinson Hyperlegible Mono",
        ["AtkinsonHyperlegibleMono[wght].ttf"]
    ),
    font!("space-mono", "Space Mono", "Space Mono", ["SpaceMono-Regular.ttf", "SpaceMono-Bold.ttf"]),
    font!("google-sans-code", "Google Sans Code", "Google Sans Code", ["GoogleSansCode[wght].ttf"]),
    // The release files register one family per weight; only 400 ships.
    font!("commit-mono", "CommitMono 400", "Commit Mono", ["CommitMono-400-Regular.ttf"]),
    font!("monaspace-neon", "Monaspace Neon Var", "Monaspace Neon", ["Monaspace Neon Var.ttf"]),
];

/// Obel Mono with `ss01` (masking) on by default: the code font while
/// "Hide secrets in code" is on and Obel Mono is chosen.
pub const OBEL_MONO_MASKED: FontInfo =
    font!("obel-mono", "Obel Mono Masked", "Obel Mono Masked", ["ObelMonoMasked[wght].ttf"]);

/// The font sizes offered (percent of the default); Ctrl+= / Ctrl+- step
/// through them.
pub const SIZES: [u16; 7] = [80, 90, 100, 110, 120, 130, 140];
pub const DEFAULT_SIZE: u16 = 100;

/// The font choices (a GPUI global).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fonts {
    pub ui: &'static FontInfo,
    pub mono: &'static FontInfo,
    pub mask_secrets: bool,
    /// Percent of the default text size, one of [`SIZES`].
    pub size: u16,
}

impl Global for Fonts {}

impl Default for Fonts {
    fn default() -> Self {
        Self { ui: &UI_FONTS[0], mono: &CODE_FONTS[0], mask_secrets: true, size: DEFAULT_SIZE }
    }
}

/// A saved font id (JSON string) in `list`; else the list's first (default).
fn find(list: &'static [FontInfo], value: Option<&str>) -> &'static FontInfo {
    value
        .and_then(|v| serde_json::from_str::<String>(v).ok())
        .and_then(|id| list.iter().find(|f| f.id == id))
        .unwrap_or(&list[0])
}

impl Fonts {
    pub fn from_settings(ui: Option<&str>, mono: Option<&str>, mask: Option<&str>, size: Option<&str>) -> Self {
        Self {
            ui: find(&UI_FONTS, ui),
            mono: find(&CODE_FONTS, mono),
            mask_secrets: mask.and_then(|v| serde_json::from_str::<bool>(v).ok()).unwrap_or(true),
            size: size
                .and_then(|v| serde_json::from_str::<u16>(v).ok())
                .filter(|s| SIZES.contains(s))
                .unwrap_or(DEFAULT_SIZE),
        }
    }

    /// The rows to save: `ui_font`, `mono_font`, `mask_secrets`, `font_size`.
    pub fn settings(&self) -> [(&'static str, String); 4] {
        [
            ("ui_font", serde_json::to_string(self.ui.id).expect("a string")),
            ("mono_font", serde_json::to_string(self.mono.id).expect("a string")),
            ("mask_secrets", self.mask_secrets.to_string()),
            ("font_size", self.size.to_string()),
        ]
    }

    /// The next size in [`SIZES`] (`bigger`) or the one before; the same
    /// size at either end.
    pub fn step_size(&self, bigger: bool) -> u16 {
        let ix = SIZES.iter().position(|s| *s == self.size).unwrap_or(2);
        let ix = if bigger { (ix + 1).min(SIZES.len() - 1) } else { ix.saturating_sub(1) };
        SIZES[ix]
    }

    /// Whether the code font can hide secrets (only Obel Mono can).
    pub fn can_mask(&self) -> bool {
        self.mono.id == CODE_FONTS[0].id
    }

    /// The font code is drawn in: Obel Mono Masked while hiding secrets.
    pub fn code(&self) -> &'static FontInfo {
        if self.can_mask() && self.mask_secrets { &OBEL_MONO_MASKED } else { self.mono }
    }
}

/// The family names registered so far.
#[derive(Default)]
struct Registered(HashSet<&'static str>);

impl Global for Registered {}

/// Register `font` with the text system, once.
fn register(font: &'static FontInfo, cx: &mut App) {
    let registered = cx.default_global::<Registered>();
    if !registered.0.insert(font.family) {
        return;
    }
    let files = font.files.iter().map(|f| Cow::Borrowed(*f)).collect();
    if let Err(e) = cx.text_system().add_fonts(files) {
        eprintln!("forkline: cannot load the font {}: {e:#}", font.title);
    }
}

/// Register every bundled font (the picker shows each name in its font).
pub fn load_all(cx: &mut App) {
    for font in UI_FONTS.iter().chain(&CODE_FONTS).chain([&OBEL_MONO_MASKED]) {
        register(font, cx);
    }
}

/// Register the chosen fonts ([`Fonts`] global, else the defaults).
pub fn register_chosen(cx: &mut App) -> Fonts {
    let fonts = cx.try_global::<Fonts>().copied().unwrap_or_default();
    register(fonts.ui, cx);
    register(fonts.code(), cx);
    fonts
}

/// Shape a sample line in each weight of the chosen fonts on a background
/// thread. DirectWrite reads an in-memory font's tables on first use, which
/// put about 12 ms into the first frame; this overlaps it with opening the
/// window. The first frame still shapes on demand if this is not done yet.
pub fn prewarm(fonts: &Fonts, cx: &App) {
    use gpui_kit::{FontWeight, TextRun, WindowTextSystem, font, hsla, px};
    let text_system = cx.text_system().clone();
    let (ui, mono) = (fonts.ui.family, fonts.code().family);
    cx.background_executor()
        .spawn(async move {
            let shaper = WindowTextSystem::new(text_system);
            let weights = [FontWeight::NORMAL, FontWeight::MEDIUM, FontWeight::SEMIBOLD, FontWeight::BOLD];
            let fonts = weights.map(|w| (ui, w)).into_iter().chain([(mono, FontWeight::NORMAL), (mono, FontWeight::BOLD)]);
            let sample = "The quick brown fox jumps over the lazy dog 0123456789 #@{}";
            for (family, weight) in fonts {
                let run = TextRun {
                    len: sample.len(),
                    font: gpui_kit::Font { weight, ..font(family) },
                    color: hsla(0., 0., 0., 1.),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                shaper.layout_line(sample, px(13.), &[run], None);
            }
        })
        .detach();
}

/// Make `fonts` the kit theme's interface and code fonts, and scale its base
/// size (the rem) by the font size.
pub fn write_kit(fonts: &Fonts, t: &mut Theme) {
    t.font_family = fonts.ui.family.into();
    t.mono_font_family = fonts.code().family.into();
    t.font_size = crate::theme::TEXT_BODY * (f32::from(fonts.size) / 100.);
}

/// Register the chosen fonts and show them (the colours stay).
#[cfg(test)]
pub fn apply(cx: &mut App) {
    let fonts = register_chosen(cx);
    Theme::update(cx, |t| write_kit(&fonts, t));
}

#[cfg(test)]
mod tests;
