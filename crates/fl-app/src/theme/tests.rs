use gpui_kit::{Hsla, hsla};

use super::*;

/// Every built-in theme, and the accent palette for every hue in steps of 5
/// degrees, both modes, both chroma levels and both tint settings; each with
/// a name for failure messages.
fn palettes() -> Vec<(String, Palette)> {
    let mut all: Vec<_> = THEMES.iter().map(|p| (p.name.to_owned(), **p)).collect();
    for hue in (0..360).step_by(5) {
        for muted in [false, true] {
            for tint in [false, true] {
                for dark in [true, false] {
                    let accent = Accent { hue, muted, tint };
                    all.push((format!("{accent:?} dark {dark}"), derive(accent, dark)));
                }
            }
        }
    }
    all
}

#[track_caller]
fn at_least(min: f32, fg: Hsla, bg: Hsla, what: &str, who: &str) {
    let ratio = contrast(fg, bg);
    assert!(ratio >= min, "{who}: {what} is {ratio:.2}:1, needs {min}:1");
}

#[test]
fn text_passes_on_every_surface() {
    for (who, p) in palettes() {
        let who = who.as_str();
        for (bg, name) in [(p.bg, "bg"), (p.surface, "surface"), (p.selected, "selected"), (p.raised, "raised")] {
            at_least(7., p.text, bg, &format!("text on {name}"), who);
            at_least(4.5, p.text_muted, bg, &format!("muted text on {name}"), who);
            at_least(4.5, p.text_read, bg, &format!("read title on {name}"), who);
        }
        at_least(7., p.text_body, p.bg, "comment body", who);
    }
}

#[test]
fn accent_passes() {
    for (who, p) in palettes() {
        let who = who.as_str();
        for (bg, name) in [(p.bg, "bg"), (p.surface, "surface"), (p.selected, "selected")] {
            at_least(3., p.accent, bg, &format!("accent on {name}"), who);
            at_least(4.5, p.accent_text, bg, &format!("accent text on {name}"), who);
        }
        at_least(4.5, p.on_accent, p.accent_fill, "button text on accent", who);
        at_least(4.5, p.on_accent, p.accent_fill_hover, "button text on hovered accent", who);
    }
}

#[test]
fn state_and_diff_colours_pass() {
    for (who, p) in palettes() {
        let who = who.as_str();
        for (s, name) in [(p.open, "open"), (p.merged, "merged"), (p.closed, "closed"), (p.pending, "pending"), (p.neutral, "neutral")] {
            for (bg, bg_name) in [(p.bg, "bg"), (p.surface, "surface"), (p.selected, "selected")] {
                at_least(3., s.fg, bg, &format!("{name} on {bg_name}"), who);
            }
            at_least(4.5, s.on_fill, s.fill, &format!("{name} pill"), who);
        }
        at_least(3., p.added, p.bg, "added count", who);
        at_least(3., p.removed, p.bg, "removed count", who);
        at_least(4.5, p.diff_add_fg, p.diff_add_bg, "added line", who);
        at_least(4.5, p.diff_del_fg, p.diff_del_bg, "removed line", who);
        at_least(3., p.diff_gutter, p.bg, "line numbers", who);
    }
}

#[test]
fn syntax_colours_read_on_every_diff_background() {
    for (who, p) in palettes() {
        let who = who.as_str();
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
                at_least(3., c, bg, &format!("{name} on {bg_name}"), who);
            }
        }
    }
}

#[test]
fn avatars_and_labels_pass_for_every_hue() {
    for (who, p) in palettes() {
        let who = who.as_str();
        for login in ["jaival", "lena-dev", "m-hoff", "sophie-k", "zed-industries", "longbridge", "a", "bot[bot]", ""] {
            let (bg, fg) = p.avatar(login);
            at_least(4.5, fg, bg, &format!("avatar of {login:?}"), who);
        }
        for hue in (0..360).step_by(5) {
            for (s, l) in [(0.9, 0.5), (0.5, 0.85), (0.7, 0.2), (0.05, 0.5), (0., 1.), (0., 0.)] {
                let (_, text) = p.label(hsla(hue as f32 / 360., s, l, 1.));
                at_least(4.5, text, p.bg, &format!("label hue {hue} s {s} l {l}"), who);
            }
        }
    }
}

#[test]
fn every_repo_colour_passes_on_every_theme() {
    for (who, p) in palettes() {
        let who = who.as_str();
        let accent = p.accent.h * 360.;
        for (i, (_, name)) in REPO_HUES.iter().enumerate() {
            let c = p.repo(i as u8);
            for (bg, bg_name) in [(p.bg, "bg"), (p.surface, "surface"), (p.selected, "selected"), (p.control, "menu")] {
                at_least(3., c, bg, &format!("repo {name} on {bg_name}"), who);
            }
            let d = ((c.h * 360. - accent + 540.) % 360. - 180.).abs();
            assert!(d >= REPO_ACCENT_GAP - 0.01, "{who}: repo {name} is {d:.1} degrees from the accent");
            // Near the accent's hue, the tone differs clearly.
            if d < REPO_ACCENT_NEAR {
                let tone = (c.s - p.accent.s).abs().max((c.l - p.accent.l).abs());
                assert!(tone >= 0.15, "{who}: repo {name} looks like the accent ({c:?} vs {:?})", p.accent);
            }
        }
    }
}

#[test]
fn a_repo_colour_comes_from_its_name_unless_picked() {
    let mut colors = RepoColors::default();
    let auto = colors.index("acme/rocket");
    assert_eq!(auto, RepoColors::new(vec![]).index("acme/rocket"), "the same on every run");
    assert!((auto as usize) < REPO_HUES.len());
    let other = (auto + 1) % REPO_HUES.len() as u8;
    colors.set("acme/rocket", Some(other));
    assert_eq!(colors.index("acme/rocket"), other);
    assert_eq!(colors.picked("acme/rocket"), Some(other));
    colors.set("acme/rocket", None);
    assert_eq!(colors.index("acme/rocket"), auto);
    // Different names spread over the hues.
    let used: std::collections::HashSet<u8> = (0..50).map(|n| colors.index(&format!("forkline-demo/project-{n:03}"))).collect();
    assert!(used.len() >= 8, "{used:?}");
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
    let c = Choices::from_settings(Some(&theme("graphite")), Some(&theme("paper")), None);
    assert_eq!((c.dark.name, c.light.name), ("graphite", "paper"));
    for p in THEMES {
        let c = Choices::from_settings(Some(&theme(p.name)), Some(&theme(p.name)), None);
        let picked = if p.dark { c.dark } else { c.light };
        assert_eq!(picked.name, p.name);
    }
    let defaults = Choices::default();
    for bad in [None, Some("not json"), Some("{}"), Some(r#"{"kind":"sepia"}"#), Some(theme("sepia").as_str())] {
        assert_eq!(Choices::from_settings(bad, bad, None), defaults, "{bad:?}");
    }
    // A theme of the other mode is ignored.
    assert_eq!(Choices::from_settings(Some(&theme("paper")), Some(&theme("forest")), None), defaults);
}

#[test]
fn the_accent_choice_loads_the_derived_palette() {
    let kind = Some(r#"{"kind":"accent"}"#);
    let teal = Some(r#"{"hue":185,"chroma":"muted","tint":false}"#);
    let c = Choices::from_settings(kind, kind, teal);
    let accent = Accent { hue: 185, muted: true, tint: false };
    assert_eq!(c.dark, derive(accent, true));
    assert_eq!(c.light, derive(accent, false));
    assert!(c.dark.dark && !c.light.dark);
    // Without an `accent` setting: indigo.
    let c = Choices::from_settings(kind, None, None);
    assert_eq!(c.dark, derive(Accent::default(), true));
    assert_eq!(c.light, DAYLIGHT);
}

#[test]
fn the_accent_setting_parses_and_falls_back_to_indigo() {
    assert_eq!(Accent::default(), Accent { hue: 268, muted: false, tint: true });
    assert_eq!(
        Accent::from_setting(Some(r#"{"hue":0,"chroma":"normal","tint":false}"#)),
        Accent { hue: 0, muted: false, tint: false }
    );
    assert_eq!(
        Accent::from_setting(Some(r#"{"hue":359,"chroma":"muted","tint":true}"#)),
        Accent { hue: 359, muted: true, tint: true }
    );
    for bad in [
        None,
        Some("not json"),
        Some("{}"),
        Some("268"),
        Some(r#"{"hue":360,"chroma":"normal","tint":true}"#),
        Some(r#"{"hue":-5,"chroma":"normal","tint":true}"#),
        Some(r#"{"hue":12.5,"chroma":"normal","tint":true}"#),
        Some(r#"{"hue":"12","chroma":"normal","tint":true}"#),
        Some(r#"{"hue":12,"chroma":"loud","tint":true}"#),
        Some(r#"{"hue":12,"chroma":"normal"}"#),
        Some(r#"{"hue":12,"chroma":"normal","tint":"yes"}"#),
    ] {
        assert_eq!(Accent::from_setting(bad), Accent::default(), "{bad:?}");
    }
}

#[test]
fn the_derived_accent_keeps_the_chosen_hue() {
    // OKLCH hue 268 is an indigo; in HSL it sits near 230-250.
    let p = derive(Accent::default(), true);
    let h = p.accent.h * 360.;
    assert!((220. ..=260.).contains(&h), "{h}");
    // Muted is calmer than normal; tint off gives grey backgrounds.
    assert!(derive(Accent { muted: true, ..Accent::default() }, true).accent.s < p.accent.s);
    assert!(derive(Accent { tint: false, ..Accent::default() }, true).bg.s < 0.01);
}

#[gpui_kit::test]
fn apply_uses_the_chosen_theme_for_each_mode(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_global(Choices { dark: FJORD, light: PAPER, ..Choices::default() });
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

#[test]
fn choices_round_trip_through_their_settings() {
    let mut c = Choices::default();
    assert_eq!((c.pick(true), c.pick(false)), (Pick::Theme("midnight"), Pick::Theme("daylight")));
    c.set(true, Pick::Theme("forest"));
    c.set(false, Pick::Theme("forest")); // a dark theme: the light choice stays
    assert_eq!(c.light.name, "daylight");
    c.set(false, Pick::Accent);
    c.set_accent(Accent { hue: 25, muted: true, tint: false });
    assert_eq!(c.light, derive(Accent { hue: 25, muted: true, tint: false }, false));
    assert_eq!(c.dark.name, "forest");
    c.mode = Some(ThemeMode::Light);
    let rows = c.settings();
    let get = |key: &str| rows.iter().find(|(k, _)| *k == key).and_then(|(_, v)| v.clone());
    let loaded = Choices {
        mode: mode_from_setting(get("theme").as_deref()),
        ..Choices::from_settings(get("colors_dark").as_deref(), get("colors_light").as_deref(), get("accent").as_deref())
    };
    assert_eq!(loaded, c);
    c.mode = None;
    assert_eq!(c.settings()[3], ("theme", None));
}
