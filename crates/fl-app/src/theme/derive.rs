//! The custom accent: one hue, from which a whole [`Palette`] is worked out.
//!
//! Surfaces, text and the accent come from per-mode lightness targets in
//! OKLCH (values: the "Custom accent" board, `docs/design/mockups/`). Then a
//! contrast pass moves any token that misses its rule away from the
//! backgrounds: text >= 7:1, muted text >= 4.5:1, accent >= 3:1 (accent
//! text >= 4.5:1), button text on the accent >= 4.5:1. GitHub state, diff and
//! syntax colours keep the hues of Midnight / Daylight and are lifted the same
//! way until they read on the derived backgrounds.

use gpui_kit::{Hsla, Rgba};

use super::{DAYLIGHT, MIDNIGHT, Palette, REPO_HUES, StateColor, contrast, hsl};

/// The saved `accent` setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accent {
    /// 0-359 degrees (OKLCH hue).
    pub hue: u16,
    /// `"chroma": "muted"`: a calmer accent and less tinted selection.
    pub muted: bool,
    /// Tint backgrounds and text with the accent's hue (off = neutral grey).
    pub tint: bool,
}

impl Default for Accent {
    /// Indigo.
    fn default() -> Self {
        Self { hue: 268, muted: false, tint: true }
    }
}

impl Accent {
    /// From the saved value, for example `{"hue":268,"chroma":"normal","tint":true}`.
    /// Missing or malformed values (also a hue outside 0-359) load as the default.
    pub fn from_setting(value: Option<&str>) -> Self {
        let parse = || -> Option<Self> {
            let v: serde_json::Value = serde_json::from_str(value?).ok()?;
            let hue = u16::try_from(v["hue"].as_u64()?).ok().filter(|h| *h < 360)?;
            let muted = match v["chroma"].as_str()? {
                "normal" => false,
                "muted" => true,
                _ => return None,
            };
            Some(Self { hue, muted, tint: v["tint"].as_bool()? })
        };
        parse().unwrap_or_default()
    }
}

/// A colour in OKLCH: lightness 0-1, chroma, hue in degrees.
#[derive(Clone, Copy, Debug)]
struct Lch(f32, f32, f32);

impl Lch {
    /// The sRGB colour; chroma is reduced until it fits in sRGB.
    fn hsla(self) -> Hsla {
        let Lch(l, c, h) = self;
        let rgb = |c: f32| oklab_to_srgb(l, c * h.to_radians().cos(), c * h.to_radians().sin());
        let in_gamut = |v: [f32; 3]| v.iter().all(|x| (-0.0005..=1.0005).contains(x));
        let mut v = rgb(c);
        if !in_gamut(v) {
            let (mut lo, mut hi) = (0., c);
            for _ in 0..24 {
                let mid = (lo + hi) / 2.;
                if in_gamut(rgb(mid)) { lo = mid } else { hi = mid }
            }
            v = rgb(lo);
        }
        let [r, g, b] = v.map(|x| x.clamp(0., 1.));
        Rgba { r, g, b, a: 1. }.into()
    }
}

/// OKLab -> gamma-encoded sRGB (Björn Ottosson's matrices); not clamped.
fn oklab_to_srgb(l: f32, a: f32, b: f32) -> [f32; 3] {
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_35 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let lin = [
        4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3,
        -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_4 * s3,
        -0.004_196_086 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3,
    ];
    lin.map(|v| if v <= 0.003_130_8 { 12.92 * v } else { 1.055 * v.max(0.).powf(1. / 2.4) - 0.055 })
}

/// Move `c`'s OKLCH lightness away from the backgrounds (up in dark mode,
/// down in light mode) until it reaches `min`:1 on every one of them.
fn fit(c: Lch, against: &[Hsla], min: f32, dark: bool) -> Hsla {
    let step = if dark { 0.005 } else { -0.005 };
    let mut c = c;
    loop {
        let out = c.hsla();
        if against.iter().all(|bg| contrast(out, *bg) >= min) || !(0. ..=1.).contains(&(c.0 + step)) {
            return out;
        }
        c.0 += step;
    }
}

/// The same for a fixed colour, in HSL lightness so its hue and saturation stay.
fn fit_hsla(c: Hsla, against: &[Hsla], min: f32, dark: bool) -> Hsla {
    let step = if dark { 0.005 } else { -0.005 };
    let mut c = c;
    while !against.iter().all(|bg| contrast(c, *bg) >= min) && (0. ..=1.).contains(&(c.l + step)) {
        c.l += step;
    }
    c
}

/// The full palette for `accent` in dark or light mode.
pub fn derive(accent: Accent, dark: bool) -> Palette {
    let h = f32::from(accent.hue);
    let k = if accent.tint { 1. } else { 0. };
    // Accent chroma and the selection's tint, normal / muted.
    let (ac, sel_c) = match (dark, accent.muted) {
        (true, false) => (0.16, 0.06),
        (true, true) => (0.09, 0.035),
        (false, false) => (0.17, 0.035),
        (false, true) => (0.10, 0.02),
    };
    let tone = |l: f32, c: f32| Lch(l, c * k, h);
    let base = if dark { &MIDNIGHT } else { &DAYLIGHT };
    let mut p = *base;
    p.name = "accent";

    // Surfaces.
    let alpha = |c: Lch, a: f32| Hsla { a, ..c.hsla() };
    if dark {
        p.bg = tone(0.19, 0.014).hsla();
        p.surface = tone(0.165, 0.014).hsla();
        p.raised = tone(0.245, 0.016).hsla();
        p.control = tone(0.225, 0.014).hsla();
        p.hover = tone(0.23, 0.014).hsla();
        p.border = tone(0.27, 0.014).hsla();
        p.border_strong = tone(0.32, 0.014).hsla();
        p.backdrop = alpha(tone(0.08, 0.02), 0.55);
    } else {
        p.bg = tone(0.995, 0.002).hsla();
        p.surface = tone(0.972, 0.008).hsla();
        p.raised = tone(0.935, 0.012).hsla();
        p.control = p.bg;
        p.hover = tone(0.96, 0.01).hsla();
        p.border = tone(0.91, 0.01).hsla();
        p.border_strong = tone(0.86, 0.012).hsla();
        p.backdrop = alpha(tone(0.2, 0.02), 0.35);
    }
    p.selected = Lch(if dark { 0.29 } else { 0.95 }, sel_c, h).hsla();

    // Text: the board's targets, then the contrast pass.
    let panes = [p.bg, p.surface, p.selected, p.raised];
    let t = |dark_l: f32, light_l: f32, c: f32| tone(if dark { dark_l } else { light_l }, c);
    p.text_strong = fit(t(0.96, 0.18, 0.006), &panes, 7., dark);
    p.text = fit(t(0.94, 0.22, 0.006), &panes, 7., dark);
    p.text_body = fit(t(0.87, 0.30, 0.008), &[p.bg], 7., dark);
    p.text_read = fit(t(0.81, 0.36, 0.01), &panes, 4.5, dark);
    p.text_muted = fit(t(0.71, 0.50, 0.014), &panes, 4.5, dark);
    p.text_faint = fit(t(0.58, 0.62, 0.012), &[p.bg], 3., dark);

    // Accent.
    let lines = [p.bg, p.surface, p.selected];
    let (al, atl) = if dark { (0.70, 0.78) } else { (0.55, 0.50) };
    p.accent = fit(Lch(al, ac, h), &lines, 3., dark);
    p.accent_text = fit(Lch(atl, ac * if dark { 0.8 } else { 1. }, h), &lines, 4.5, dark);
    // Button: white text on a deep enough fill, else near-black text.
    let white = hsl(0., 0., 100.);
    let ink = Lch(0.2, 0.03, h).hsla();
    if dark {
        p.accent_fill = fit(Lch(al, ac, h), &[ink], 4.5, true);
        p.accent_fill_hover = fit(Lch(al + 0.05, ac, h), &[ink], 4.5, true);
        p.on_accent = ink;
    } else {
        p.accent_fill = fit(Lch(al, ac, h), &[white], 4.5, false);
        p.accent_fill_hover = fit(Lch(al - 0.05, ac, h), &[white], 4.5, false);
        p.on_accent = white;
    }

    // Fixed hues, lifted until they read on the derived backgrounds.
    let up = |c: Hsla, against: &[Hsla], min: f32| fit_hsla(c, against, min, dark);
    p.star = up(p.star, &lines, 3.);
    for s in [&mut p.open, &mut p.merged, &mut p.closed, &mut p.pending] {
        *s = StateColor { fg: up(s.fg, &lines, 3.), ..*s };
    }
    p.neutral = StateColor { fg: p.text_muted, fill: p.raised, on_fill: p.text_body };
    p.added = up(p.added, &[p.bg], 3.);
    p.removed = up(p.removed, &[p.bg], 3.);
    p.diff_hunk_bg = Lch(if dark { 0.24 } else { 0.97 }, 0.03, h).hsla();
    p.diff_gutter = fit(t(0.62, 0.55, 0.012), &[p.bg, p.diff_hunk_bg], 3., dark);
    let code_bgs = [p.bg, p.diff_add_bg, p.diff_del_bg, p.raised];
    let s = &mut p.syntax;
    for c in [&mut s.keyword, &mut s.string, &mut s.function, &mut s.type_, &mut s.number, &mut s.property] {
        *c = up(*c, &code_bgs, 3.);
    }
    s.comment = fit(t(0.64, 0.52, 0.012), &code_bgs, 3., dark);
    s.punctuation = fit(t(0.74, 0.45, 0.012), &code_bgs, 3., dark);
    p.scrollbar_thumb = Hsla { a: if dark { 0.35 } else { 0.3 }, ..p.text_muted };

    // Repo dots: the base lightness, moved until every hue passes.
    let dots = [p.bg, p.surface, p.selected, p.control];
    let step = if dark { 1. } else { -1. };
    let passes = |p: &Palette| (0..REPO_HUES.len() as u8).all(|i| dots.iter().all(|bg| contrast(p.repo(i), *bg) >= 3.));
    while !passes(&p) && (0. ..=100.).contains(&(p.repo.1 + step)) {
        p.repo.1 += step;
    }
    p
}
