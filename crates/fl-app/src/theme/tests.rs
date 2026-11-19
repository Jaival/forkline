use gpui_kit::{Hsla, hsla};

use super::*;

const PALETTES: [&Palette; 8] = THEMES;

#[track_caller]
fn at_least(min: f32, fg: Hsla, bg: Hsla, what: &str, p: &Palette) {
    let ratio = contrast(fg, bg);
    assert!(ratio >= min, "{}: {what} is {ratio:.2}:1, needs {min}:1", p.name);
}

#[test]
fn text_passes_on_every_surface() {
    for p in PALETTES {
        for (bg, name) in [(p.bg, "bg"), (p.surface, "surface"), (p.selected, "selected"), (p.raised, "raised")] {
            at_least(7., p.text, bg, &format!("text on {name}"), p);
            at_least(4.5, p.text_muted, bg, &format!("muted text on {name}"), p);
            at_least(4.5, p.text_read, bg, &format!("read title on {name}"), p);
        }
        at_least(7., p.text_body, p.bg, "comment body", p);
    }
}

#[test]
fn accent_passes() {
    for p in PALETTES {
        for (bg, name) in [(p.bg, "bg"), (p.surface, "surface"), (p.selected, "selected")] {
            at_least(3., p.accent, bg, &format!("accent on {name}"), p);
            at_least(4.5, p.accent_text, bg, &format!("accent text on {name}"), p);
        }
        at_least(4.5, p.on_accent, p.accent_fill, "button text on accent", p);
        at_least(4.5, p.on_accent, p.accent_fill_hover, "button text on hovered accent", p);
    }
}

#[test]
fn state_and_diff_colours_pass() {
    for p in PALETTES {
        for (s, name) in [(p.open, "open"), (p.merged, "merged"), (p.closed, "closed"), (p.pending, "pending"), (p.neutral, "neutral")] {
            for (bg, bg_name) in [(p.bg, "bg"), (p.surface, "surface"), (p.selected, "selected")] {
                at_least(3., s.fg, bg, &format!("{name} on {bg_name}"), p);
            }
            at_least(4.5, s.on_fill, s.fill, &format!("{name} pill"), p);
        }
        at_least(3., p.added, p.bg, "added count", p);
        at_least(3., p.removed, p.bg, "removed count", p);
        at_least(4.5, p.diff_add_fg, p.diff_add_bg, "added line", p);
        at_least(4.5, p.diff_del_fg, p.diff_del_bg, "removed line", p);
        at_least(3., p.diff_gutter, p.bg, "line numbers", p);
    }
}

#[test]
fn syntax_colours_read_on_every_diff_background() {
    for p in PALETTES {
        let s = &p.syntax;
        let colours = [
            (s.keyword, "keyword"),
            (s.string, "string"),
            (s.comment, "comment"),
            (s.function, "function"),
            (s.type_, "type"),
            (s.number, "number"),
            (s.property, "property"),
            (s.punctuation, "punctuation"),
        ];
        for (c, name) in colours {
            for (bg, bg_name) in [(p.bg, "bg"), (p.diff_add_bg, "added"), (p.diff_del_bg, "removed"), (p.raised, "code block")] {
                at_least(3., c, bg, &format!("{name} on {bg_name}"), p);
            }
        }
    }
}

#[test]
fn avatars_and_labels_pass_for_every_hue() {
    for p in PALETTES {
        for login in ["jaival", "lena-dev", "m-hoff", "sophie-k", "zed-industries", "longbridge", "a", "bot[bot]", ""] {
            let (bg, fg) = p.avatar(login);
            at_least(4.5, fg, bg, &format!("avatar of {login:?}"), p);
        }
        for hue in (0..360).step_by(5) {
            for (s, l) in [(0.9, 0.5), (0.5, 0.85), (0.7, 0.2), (0.05, 0.5), (0., 1.), (0., 0.)] {
                let (_, text) = p.label(hsla(hue as f32 / 360., s, l, 1.));
                at_least(4.5, text, p.bg, &format!("label hue {hue} s {s} l {l}"), p);
            }
        }
    }
}

#[test]
fn an_avatar_colour_is_stable() {
    // FNV-1a, not std's randomly seeded hasher.
    assert_eq!(stable_hash("jaival"), stable_hash("jaival"));
    assert_eq!(stable_hash(""), 0x811c_9dc5);
    assert_eq!(MIDNIGHT.avatar("lena-dev"), MIDNIGHT.avatar("lena-dev"));
}

#[test]
fn every_theme_has_a_unique_name_and_two_are_light() {
    let mut names: Vec<_> = THEMES.iter().map(|p| p.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), THEMES.len());
    let light: Vec<_> = THEMES.iter().filter(|p| !p.dark).map(|p| p.name).collect();
    assert_eq!(light, ["daylight", "paper"]);
}

#[test]
fn saved_theme_choices_parse_and_fall_back_to_the_defaults() {
    let theme = |name: &str| format!(r#"{{"kind":"theme","name":"{name}"}}"#);
    let c = Choices::from_settings(Some(&theme("graphite")), Some(&theme("paper")));
    assert_eq!((c.dark.name, c.light.name), ("graphite", "paper"));
    for p in THEMES {
        let c = Choices::from_settings(Some(&theme(p.name)), Some(&theme(p.name)));
        let picked = if p.dark { c.dark } else { c.light };
        assert_eq!(picked.name, p.name);
    }
    let defaults = Choices::default();
    for bad in [None, Some("not json"), Some("{}"), Some(r#"{"kind":"accent"}"#), Some(theme("sepia").as_str())] {
        assert_eq!(Choices::from_settings(bad, bad), defaults, "{bad:?}");
    }
    // A theme of the other mode is ignored.
    assert_eq!(Choices::from_settings(Some(&theme("paper")), Some(&theme("forest"))), defaults);
}

#[gpui_kit::test]
fn apply_uses_the_chosen_theme_for_each_mode(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_global(Choices { dark: &FJORD, light: &PAPER });
        apply(Some(ThemeMode::Dark), None, cx);
        assert_eq!(cx.palette().name, "fjord");
        assert_eq!(Theme::global(cx).background, FJORD.bg);
        apply(Some(ThemeMode::Light), None, cx);
        assert_eq!(cx.palette().name, "paper");
        assert_eq!(Theme::global(cx).background, PAPER.bg);
    });
}

#[test]
fn saved_mode_parses_and_falls_back_to_system() {
    assert_eq!(mode_from_setting(Some("\"dark\"")), Some(ThemeMode::Dark));
    assert_eq!(mode_from_setting(Some("\"light\"")), Some(ThemeMode::Light));
    assert_eq!(mode_from_setting(Some("\"sepia\"")), None);
    assert_eq!(mode_from_setting(Some("not json")), None);
    assert_eq!(mode_from_setting(None), None);
}
