use gpui_kit::component::ThemeMode;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use crate::test_support::{open, press, type_text};
use crate::theme::{ActivePalette as _, Choices, Pick};
use crate::workspace::Workspace;

fn start(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open(cx, dir.path());
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    (handle, ws, dir)
}

fn click(cx: &mut TestAppContext, handle: AnyWindowHandle, id: impl Into<gpui_kit::ElementId>) {
    let id = id.into();
    cx.update_window(handle, |_, window, cx| {
        window.click(id, cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn open_dialog(cx: &mut TestAppContext, handle: AnyWindowHandle, ws: &Entity<Workspace>) {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open_appearance(window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn is_open(cx: &mut TestAppContext, ws: &Entity<Workspace>) -> bool {
    cx.read(|cx| ws.read(cx).appearance().is_some())
}

fn palette_name(cx: &mut TestAppContext) -> &'static str {
    cx.read(|cx| cx.palette().name)
}

fn setting(cx: &mut TestAppContext, key: &str) -> Option<String> {
    cx.read(|cx| {
        crate::store::Store::global(cx).read(cx).db().read(|c| fl_db::queries::layout::setting(c, key)).unwrap()
    })
}

/// Wait for the background settings write.
fn wait_for_setting(cx: &mut TestAppContext, key: &str, want: Option<&str>) {
    crate::test_support::wait_for(cx, key, |cx| setting(cx, key).as_deref() == want);
}

#[gpui_kit::test]
fn the_rail_menu_sets_the_mode_and_opens_appearance(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    // Menu: System 0, Light 1, Dark 2, separator 3, Appearance… 4.
    let pick = |cx: &mut TestAppContext, ix: usize| {
        click(cx, handle, "nav-theme");
        cx.update_window(handle, |_, window, cx| {
            window.within("popup-menu").click(ix, cx);
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
    };
    pick(cx, 1);
    assert_eq!(palette_name(cx), "daylight");
    assert_eq!(cx.read(|cx| cx.global::<Choices>().mode), Some(ThemeMode::Light));
    wait_for_setting(cx, "theme", Some("\"light\""));
    pick(cx, 2);
    assert_eq!(palette_name(cx), "midnight");
    wait_for_setting(cx, "theme", Some("\"dark\""));
    pick(cx, 0);
    assert_eq!(cx.read(|cx| cx.global::<Choices>().mode), None);
    wait_for_setting(cx, "theme", None);
    pick(cx, 4);
    assert!(is_open(cx, &ws));
}

#[gpui_kit::test]
fn ctrl_k_opens_appearance(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    press(cx, handle, "ctrl-k");
    type_text(cx, handle, "appearance");
    press(cx, handle, "enter");
    assert!(is_open(cx, &ws));
    assert!(cx.read(|cx| ws.read(cx).palette().is_none()));
}

#[gpui_kit::test]
fn changes_apply_live(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    open_dialog(cx, handle, &ws);
    // Dark cards: Midnight 0, Graphite 1, Fjord 2, ...; the accent is last (6).
    click(cx, handle, ("appearance-card", 1usize));
    assert_eq!(palette_name(cx), "graphite");
    // A preset swatch (Green 150) switches this mode to the accent.
    click(cx, handle, ("appearance-swatch", 5usize));
    assert_eq!(palette_name(cx), "accent");
    let choices = cx.read(|cx| *cx.global::<Choices>());
    assert_eq!(choices.accent.hue, 150);
    assert_eq!(choices.pick(true), Pick::Accent);
    click(cx, handle, "appearance-tint");
    assert!(!cx.read(|cx| cx.global::<Choices>().accent.tint));
    assert!(cx.read(|cx| cx.palette().bg.s) < 0.01, "tint off gives grey backgrounds");
    // Editing the light mode shows it at once.
    click(cx, handle, "appearance-edit-light");
    click(cx, handle, ("appearance-card", 1usize));
    assert_eq!(palette_name(cx), "paper");
    // Nothing is saved while the dialog is open.
    assert_eq!(setting(cx, "colors_dark"), None);
}

#[gpui_kit::test]
fn esc_restores_the_choices_from_when_it_opened(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    // A saved Dark mode: in System mode Esc shows the test window's OS mode
    // (light), see `appearance_cancel_in_system_mode_shows_the_current_os_mode`.
    cx.update(|cx| cx.set_global(Choices { mode: Some(ThemeMode::Dark), ..Choices::default() }));
    let before = cx.read(|cx| *cx.global::<Choices>());
    open_dialog(cx, handle, &ws);
    click(cx, handle, ("appearance-card", 2usize));
    click(cx, handle, ("appearance-swatch", 9usize));
    click(cx, handle, "appearance-mode-light");
    assert_eq!(palette_name(cx), "daylight");
    press(cx, handle, "escape");
    assert!(!is_open(cx, &ws));
    assert_eq!(cx.read(|cx| *cx.global::<Choices>()), before);
    assert_eq!(palette_name(cx), "midnight");
    cx.run_until_parked();
    assert_eq!(setting(cx, "colors_dark"), None);
    assert_eq!(setting(cx, "accent"), None);
}

#[gpui_kit::test]
fn done_saves_the_settings(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    open_dialog(cx, handle, &ws);
    click(cx, handle, ("appearance-card", 2usize)); // Fjord
    click(cx, handle, "appearance-edit-light");
    click(cx, handle, ("appearance-swatch", 4usize)); // Teal 185, light mode
    click(cx, handle, "appearance-muted");
    click(cx, handle, "appearance-mode-dark");
    click(cx, handle, "appearance-done");
    assert!(!is_open(cx, &ws));
    assert_eq!(palette_name(cx), "fjord");
    wait_for_setting(cx, "theme", Some("\"dark\""));
    assert_eq!(setting(cx, "colors_dark").as_deref(), Some(r#"{"kind":"theme","name":"fjord"}"#));
    assert_eq!(setting(cx, "colors_light").as_deref(), Some(r#"{"kind":"accent"}"#));
    let accent = setting(cx, "accent");
    assert_eq!(
        crate::theme::Accent::from_setting(accent.as_deref()),
        crate::theme::Accent { hue: 185, muted: true, tint: true }
    );
    // What was saved loads as what is shown.
    let loaded = Choices {
        mode: crate::theme::mode_from_setting(setting(cx, "theme").as_deref()),
        ..Choices::from_settings(
            setting(cx, "colors_dark").as_deref(),
            setting(cx, "colors_light").as_deref(),
            accent.as_deref(),
        )
    };
    assert_eq!(loaded, cx.read(|cx| *cx.global::<Choices>()));
}

fn fonts(cx: &mut TestAppContext) -> crate::fonts::Fonts {
    cx.read(|cx| cx.try_global::<crate::fonts::Fonts>().copied().unwrap_or_default())
}

/// The kit theme's interface and code families: what the app draws with.
fn families(cx: &mut TestAppContext) -> (String, String) {
    use gpui_kit::component::ActiveTheme as _;
    cx.read(|cx| (cx.theme().font_family.to_string(), cx.theme().mono_font_family.to_string()))
}

fn has(cx: &mut TestAppContext, handle: AnyWindowHandle, id: &'static str) -> bool {
    cx.update_window(handle, |_, window, _| window.try_find(id).is_some()).unwrap()
}

#[gpui_kit::test]
fn font_changes_apply_live(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    assert_eq!(families(cx), ("Inter".into(), "Obel Mono Masked".into()));
    open_dialog(cx, handle, &ws);
    click(cx, handle, "appearance-tab-fonts");
    assert!(has(cx, handle, "appearance-sample"));
    // Interface fonts: Inter 0, Geist 1, ...; code fonts: Obel Mono 0, JetBrains Mono 1, ...
    click(cx, handle, ("appearance-ui-font", 1usize));
    assert_eq!(families(cx).0, "Geist");
    assert!(has(cx, handle, "appearance-mask"), "Obel Mono offers the switch");
    click(cx, handle, "appearance-mask");
    assert!(!fonts(cx).mask_secrets);
    assert_eq!(families(cx).1, "Obel Mono");
    click(cx, handle, "appearance-mask");
    assert_eq!(families(cx).1, "Obel Mono Masked");
    // Another code font has no masking: the switch is hidden.
    click(cx, handle, ("appearance-code-font", 1usize));
    assert_eq!(families(cx).1, "JetBrains Mono");
    assert!(!has(cx, handle, "appearance-mask"));
    // The licences replace the sample.
    click(cx, handle, "appearance-licences-toggle");
    assert!(has(cx, handle, "appearance-licences"));
    assert!(!has(cx, handle, "appearance-sample"));
    // Nothing is saved while the dialog is open.
    assert_eq!(setting(cx, "ui_font"), None);
    assert_eq!(setting(cx, "mono_font"), None);
}

#[gpui_kit::test]
fn esc_restores_the_fonts(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    let before = fonts(cx);
    open_dialog(cx, handle, &ws);
    click(cx, handle, "appearance-tab-fonts");
    click(cx, handle, ("appearance-ui-font", 5usize));
    click(cx, handle, ("appearance-code-font", 3usize));
    assert_eq!(families(cx), ("Source Serif 4".into(), "IBM Plex Mono".into()));
    press(cx, handle, "escape");
    assert!(!is_open(cx, &ws));
    assert_eq!(fonts(cx), before);
    assert_eq!(families(cx), ("Inter".into(), "Obel Mono Masked".into()));
    cx.run_until_parked();
    assert_eq!(setting(cx, "ui_font"), None);
    assert_eq!(setting(cx, "mask_secrets"), None);
}

#[gpui_kit::test]
fn done_saves_the_fonts(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    open_dialog(cx, handle, &ws);
    click(cx, handle, "appearance-tab-fonts");
    click(cx, handle, ("appearance-ui-font", 9usize)); // Manrope
    click(cx, handle, "appearance-mask");
    click(cx, handle, ("appearance-code-font", 6usize)); // Space Mono
    click(cx, handle, "appearance-done");
    assert!(!is_open(cx, &ws));
    assert_eq!(families(cx), ("Manrope".into(), "Space Mono".into()));
    wait_for_setting(cx, "ui_font", Some("\"manrope\""));
    assert_eq!(setting(cx, "mono_font").as_deref(), Some("\"space-mono\""));
    assert_eq!(setting(cx, "mask_secrets").as_deref(), Some("false"));
    // The colours are saved with them; what was saved loads as what is shown.
    assert!(setting(cx, "colors_dark").is_some());
    let loaded = crate::fonts::Fonts::from_settings(
        setting(cx, "ui_font").as_deref(),
        setting(cx, "mono_font").as_deref(),
        setting(cx, "mask_secrets").as_deref(),
    );
    assert_eq!(loaded, fonts(cx));
}

#[gpui_kit::test]
fn the_dialog_fits_a_short_window_and_its_body_scrolls(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    // The window's minimum size.
    cx.simulate_window_resize(handle, gpui_kit::size(gpui_kit::px(800.), gpui_kit::px(500.)));
    open_dialog(cx, handle, &ws);
    for tab in ["appearance-tab-colours", "appearance-tab-fonts"] {
        click(cx, handle, tab);
        let (done, body) = cx
            .update_window(handle, |_, window, _| {
                (window.find("appearance-done").bounds(), window.find("appearance-body").bounds())
            })
            .unwrap();
        assert!(done.bottom() <= gpui_kit::px(500.), "{tab}: Done is below the window: {done:?}");
        assert!(body.bottom() <= done.top(), "{tab}: the body overlaps the footer");
    }
    // The body starts at its top: the first section is shown at full height,
    // not squashed or scrolled away.
    click(cx, handle, "appearance-tab-colours");
    let (system, body) = cx
        .update_window(handle, |_, window, _| {
            (window.find("appearance-mode-system").bounds(), window.find("appearance-body").bounds())
        })
        .unwrap();
    assert!(system.top() >= body.top() && system.size.height >= gpui_kit::px(20.), "mode row {system:?} in body {body:?}");
    // The last control of the Colours tab scrolls into reach.
    click(cx, handle, "appearance-tab-colours");
    let tint = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, _| window.find("appearance-tint").bounds().top()).unwrap()
    };
    let before = tint(cx);
    cx.update_window(handle, |_, window, cx| {
        window.scroll("appearance-body", gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(-400.))), cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!(tint(cx) < before, "the body did not scroll");
}
