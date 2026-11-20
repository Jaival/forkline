use std::borrow::Cow;
use std::sync::Arc;

use gpui_kit::{Font, FontRun, FontWeight, PlatformTextSystem, px};

use super::*;
use crate::theme::TEXT_STYLES;

/// The platform's real text system (DirectWrite on Windows) with every
/// bundled font added.
fn text_system() -> Arc<dyn PlatformTextSystem> {
    let ts = gpui_kit::platform::current_platform(false).text_system();
    let files = all().iter().flat_map(|f| f.files.iter().map(|b| Cow::Borrowed(*b))).collect();
    ts.add_fonts(files).unwrap();
    ts
}

fn all() -> Vec<&'static FontInfo> {
    UI_FONTS.iter().chain(&CODE_FONTS).chain([&OBEL_MONO_MASKED]).collect()
}

#[test]
fn unknown_names_and_bad_values_load_the_defaults() {
    assert_eq!(Fonts::from_settings(None, None, None), Fonts::default());
    assert_eq!(Fonts::default().ui.id, "inter");
    assert_eq!(Fonts::default().mono.id, "obel-mono");
    assert!(Fonts::default().mask_secrets);
    for bad in ["\"comic-sans\"", "inter", "{\"id\":\"inter\"}", "42", "", "\"JetBrains Mono\""] {
        assert_eq!(Fonts::from_settings(Some(bad), Some(bad), Some(bad)), Fonts::default(), "{bad}");
    }
    // A code font is not an interface font, and the other way round.
    let swapped = Fonts::from_settings(Some("\"jetbrains-mono\""), Some("\"inter\""), None);
    assert_eq!(swapped, Fonts::default());
}

#[test]
fn settings_round_trip() {
    for (ui, mono) in UI_FONTS.iter().zip(CODE_FONTS.iter().cycle()) {
        for mask_secrets in [true, false] {
            let fonts = Fonts { ui, mono, mask_secrets };
            let [(k1, ui), (k2, mono), (k3, mask)] = fonts.settings();
            assert_eq!((k1, k2, k3), ("ui_font", "mono_font", "mask_secrets"));
            assert_eq!(Fonts::from_settings(Some(&ui), Some(&mono), Some(&mask)), fonts);
        }
    }
}

#[test]
fn only_obel_mono_hides_secrets() {
    let on = Fonts::default();
    assert!(on.can_mask());
    assert_eq!(on.code().family, "Obel Mono Masked");
    let off = Fonts { mask_secrets: false, ..on };
    assert_eq!(off.code().family, "Obel Mono");
    for mono in &CODE_FONTS[1..] {
        let other = Fonts { mono, ..on };
        assert!(!other.can_mask());
        assert_eq!(other.code(), mono, "the switch does nothing for {}", mono.title);
    }
}

#[test]
fn the_table_has_21_fonts_with_unique_ids_and_ofl_licences() {
    assert_eq!(UI_FONTS.len() + CODE_FONTS.len(), 21);
    let mut ids: Vec<&str> = UI_FONTS.iter().chain(&CODE_FONTS).map(|f| f.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 21);
    for f in all() {
        assert!(f.licence.contains("SIL Open Font License, Version 1.1"), "{}", f.title);
        assert!(!f.files.is_empty());
    }
}

#[test]
fn every_font_registers_its_family_and_weights() {
    let ts = text_system();
    let names = ts.all_font_names();
    for f in all() {
        assert!(names.iter().any(|n| n == f.family), "{} registers {:?}", f.title, f.family);
        let regular = Font { family: f.family.into(), ..Font::default() };
        let id = ts.font_id(&regular).unwrap();
        let bold = ts.font_id(&Font { weight: FontWeight::BOLD, ..regular.clone() }).unwrap();
        // Monospaced bold keeps the width, so only the interface fonts show it.
        if UI_FONTS.iter().any(|u| u.id == f.id) {
            let text = "Review requested";
            let width = |id| ts.layout_line(text, px(13.5), &[FontRun { len: text.len(), font_id: id }]).width;
            assert_ne!(width(id), width(bold), "{} has a bold weight", f.title);
        }
    }
}

/// GPUI centres a line's ascent + descent in its line height; every ASCII
/// letter's ink must stay inside that box, or a fixed-height row clips it.
#[test]
fn letters_fit_every_line_box() {
    let ts = text_system();
    let chars: Vec<char> = (' '..='~').collect();
    let text: String = chars.iter().collect();
    let mut misses = Vec::new();
    for f in all() {
        for weight in [FontWeight::NORMAL, FontWeight::SEMIBOLD] {
            let id = ts.font_id(&Font { family: f.family.into(), weight, ..Font::default() }).unwrap();
            let upm = ts.font_metrics(id).units_per_em as f32;
            // Ink above / below the baseline, in em.
            let (mut up, mut down) = (0f32, 0f32);
            for &c in &chars {
                let Some(glyph) = ts.glyph_for_char(id, c) else { continue };
                let b = ts.typographic_bounds(id, glyph).unwrap();
                up = up.max((b.origin.y + b.size.height) / upm);
                down = down.max(-b.origin.y / upm);
            }
            for (size, line) in TEXT_STYLES {
                let layout = ts.layout_line(&text, size, &[FontRun { len: text.len(), font_id: id }]);
                let pad = (line - layout.ascent - layout.descent) / 2.;
                let top = pad + layout.ascent - size * up;
                let bottom = pad + layout.ascent + size * down;
                // Half a pixel of antialiasing is not a clipped letter.
                if top < px(-0.5) || bottom > line + px(0.5) {
                    misses.push(format!("{} {:?} {size:?}/{line:?}: ink {top:?}..{bottom:?}", f.title, weight));
                }
            }
        }
    }
    assert!(misses.is_empty(), "letters outside their line box:\n{}", misses.join("\n"));
}

fn glyphs(ts: &Arc<dyn PlatformTextSystem>, font: Font, text: &str) -> (Vec<u32>, gpui_kit::Pixels) {
    let id = ts.font_id(&font).unwrap();
    let line = ts.layout_line(text, px(12.5), &[FontRun { len: text.len(), font_id: id }]);
    (line.runs.iter().flat_map(|r| r.glyphs.iter().map(|g| g.id.0)).collect(), line.width)
}

fn family(name: &'static str) -> Font {
    Font { family: name.into(), ..Font::default() }
}

/// Obel Mono Masked draws `ghp_` and four characters, then mask dots; Obel
/// Mono draws every character. The text itself never changes.
#[test]
fn the_masked_font_draws_dots_after_the_first_four_characters() {
    let ts = text_system();
    let token = format!("ghp_{}", "x7Kq2mN9pR4tW8vY3zB6cD1fH5jL0sA2eG7i");
    assert_eq!(token.len(), 4 + 36);
    let (plain, plain_width) = glyphs(&ts, family("Obel Mono"), &token);
    let (masked, masked_width) = glyphs(&ts, family("Obel Mono Masked"), &token);
    assert_eq!(plain.len(), token.len());
    assert_eq!(masked.len(), token.len());
    // `ghp` and the four kept characters are the plain glyphs.
    assert_eq!(masked[..3], plain[..3]);
    assert_eq!(masked[4..8], plain[4..8]);
    // The other 32 are one mask glyph, which the plain font never draws here.
    let dot = masked[8];
    assert!(masked[8..].iter().all(|&g| g == dot), "{masked:?}");
    assert!(!plain.contains(&dot));
    // Same advance: masking never moves the columns.
    assert_eq!(plain_width, masked_width);
}

/// DirectWrite applies `ss01` through GPUI's font features: Obel Mono with
/// `ss01` draws what Obel Mono Masked draws. (Forkline uses the second font;
/// see the module docs.)
#[test]
fn directwrite_applies_ss01() {
    let ts = text_system();
    let token = "ghp_x7Kq2mN9pR4tW8vY3zB6cD1fH5jL0sA2eG7i";
    let ss01 = gpui_kit::FontFeatures(Arc::new(vec![("ss01".into(), 1)]));
    let (with, _) = glyphs(&ts, Font { features: ss01, ..family("Obel Mono") }, token);
    let (masked, _) = glyphs(&ts, family("Obel Mono Masked"), token);
    let (plain, _) = glyphs(&ts, family("Obel Mono"), token);
    assert_eq!(with, masked);
    assert_ne!(with, plain);
}
