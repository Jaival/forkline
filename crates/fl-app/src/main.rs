//! Forkline GPUI app. See docs/adr/0001-gpui.md.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod appearance;
mod assets;
mod bench;
mod drafts;
mod fonts;
mod footer;
mod menu_preview;
mod palette;
mod panel;
mod rail;
mod repo_color;
mod signin;
mod smooth_scroll;
mod store;
mod theme;
#[cfg(test)]
mod test_support;
mod view_ref;
mod views;
mod workspace;

use std::path::PathBuf;

use fl_db::queries::layout;
use gpui_kit::{AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};

use crate::store::Store;
use crate::workspace::Workspace;

#[derive(Default)]
struct Args {
    /// `--data-dir <path>`: use this folder instead of %LOCALAPPDATA%\Forkline.
    data_dir: Option<PathBuf>,
    /// `--bench <out.json>`: offline, default layout, write timings, exit.
    bench_out: Option<PathBuf>,
    /// `--offline`: do not start sync (looking at fake data).
    offline: bool,
    /// `--appearance`: open the "Appearance…" dialog at launch (screenshots).
    appearance: bool,
    /// `--appearance-fonts`: the same, on its Fonts tab.
    appearance_fonts: bool,
    /// `--open-menu <name>`: show dropdown menus at launch (screenshots; see
    /// `menu_preview.rs`).
    open_menu: Option<&'static str>,
}

impl Args {
    fn parse(mut it: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut a = Self::default();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--data-dir" => a.data_dir = Some(it.next().ok_or("--data-dir needs a path")?.into()),
                "--offline" => a.offline = true,
                "--appearance" => a.appearance = true,
                "--appearance-fonts" => a.appearance_fonts = true,
                "--open-menu" => {
                    let name = it.next().ok_or("--open-menu needs a menu name")?;
                    let known = menu_preview::NAMES.iter().copied().find(|n| *n == name);
                    a.open_menu = Some(known.ok_or_else(|| format!("unknown menu: {name} (one of {})", menu_preview::NAMES.join(", ")))?);
                }
                "--bench" => a.bench_out = Some(it.next().ok_or("--bench needs an output path")?.into()),
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(a)
    }
}

/// The main window keeps the native title bar. A client-drawn frame (the
/// kit's `TitleBar::window_options`, a transparent title bar) slowed every
/// present on Windows: `diff_scroll_fps` 240 -> 103, `switch_tab_ms` 4 -> 10.
fn window_options(bounds: Bounds<gpui_kit::Pixels>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(800.), px(500.))),
        ..WindowOptions::default()
    }
}

fn main() {
    let main_ms = bench::epoch_ms();
    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("forkline: {e}");
            std::process::exit(2);
        }
    };
    let bench_out = args.bench_out;
    let bench = bench_out.is_some();
    let offline = bench || args.offline;
    let open_appearance = (args.appearance || args.appearance_fonts) && !bench;
    let appearance_fonts = args.appearance_fonts;
    let open_menu = args.open_menu.filter(|_| !bench);
    let data_dir = args.data_dir.unwrap_or_else(fl_db::default_data_dir);

    let app = gpui_kit::application().with_assets(assets::AppAssets);
    let app_ms = bench::epoch_ms();
    app.run(move |cx| {
        let run_ms = bench::epoch_ms();
        gpui_kit::init(cx);
        let kit_ms = bench::epoch_ms();
        panel::init(cx);
        workspace::init(cx);
        views::init(cx);
        signin::init(cx);
        appearance::init(cx);
        drafts::Drafts::init(cx);
        if let Some(out) = bench_out.clone() {
            bench::init(out, &[("main", main_ms), ("app_created", app_ms), ("run_callback", run_ms), ("kit_init", kit_ms)], cx);
        }
        if let Err(e) = Store::init(data_dir, fl_sync::Config::github(), offline, bench, cx) {
            eprintln!("forkline: cannot open the database: {e:#}");
            cx.quit();
            return;
        }
        bench::mark("store_ready", cx);

        let [mode, dark, light, accent, ui_font, mono_font, mask] = Store::global(cx)
            .read(cx)
            .db()
            .read(|c| {
                Ok(["theme", "colors_dark", "colors_light", "accent", "ui_font", "mono_font", "mask_secrets"]
                    .map(|key| layout::setting(c, key).ok().flatten()))
            })
            .unwrap_or_default();
        let mode = theme::mode_from_setting(mode.as_deref());
        cx.set_global(theme::Choices { mode, ..theme::Choices::from_settings(dark.as_deref(), light.as_deref(), accent.as_deref()) });
        cx.set_global(fonts::Fonts::from_settings(ui_font.as_deref(), mono_font.as_deref(), mask.as_deref()));
        let picked = Store::global(cx).read(cx).db().read(fl_db::queries::repo::colors).unwrap_or_default();
        cx.set_global(theme::RepoColors::new(picked));
        theme::apply(mode, None, cx);
        fonts::prewarm(cx.global::<fonts::Fonts>(), cx);

        let options = window_options(Bounds::centered(None, size(px(1400.), px(900.)), cx));
        let (handle, ws) = gpui_kit::open_window(options, cx, |window, cx| {
            window.set_window_title("Forkline");
            theme::sync_title_bar(window, cx.global::<theme::Palette>().dark);
            let ws = cx.new(|cx| Workspace::new(!bench, window, cx));
            if open_appearance {
                ws.update(cx, |ws, cx| {
                    ws.open_appearance(window, cx);
                    if appearance_fonts && let Some(dialog) = ws.appearance() {
                        dialog.update(cx, |dialog, cx| dialog.show_fonts(cx));
                    }
                });
            }
            if let Some(name) = open_menu {
                let menus = menu_preview::build(name, &ws, window, cx);
                ws.update(cx, |ws, cx| ws.preview_menus(menus, cx));
            }
            ws
        })
        .expect("cannot open the main window");
        bench::mark("window_open", cx);
        if bench {
            let _ = handle.update(cx, |_, window, cx| {
                window.spawn(cx, async move |cx| bench::run(ws, cx.clone()).await).detach();
            });
        }
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
    });
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn the_main_window_keeps_the_native_title_bar() {
        let options = super::window_options(Bounds::new(point(px(0.), px(0.)), size(px(800.), px(600.))));
        assert!(options.titlebar.is_none_or(|t| !t.appears_transparent), "a client-drawn frame halves the frame rate");
    }
}
