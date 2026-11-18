//! Forkline GPUI app. See docs/adr/0001-gpui.md.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
mod bench;
mod drafts;
mod footer;
mod palette;
mod panel;
mod rail;
mod signin;
mod store;
#[cfg(test)]
mod test_support;
mod view_ref;
mod views;
mod workspace;

use std::path::PathBuf;

use fl_db::queries::layout;
use gpui_kit::component::{Theme, ThemeMode};
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
}

impl Args {
    fn parse(mut it: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut a = Self::default();
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--data-dir" => a.data_dir = Some(it.next().ok_or("--data-dir needs a path")?.into()),
                "--offline" => a.offline = true,
                "--bench" => a.bench_out = Some(it.next().ok_or("--bench needs an output path")?.into()),
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        Ok(a)
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

        let theme = Store::global(cx).read(cx).db().read(|c| layout::setting(c, "theme")).ok().flatten();
        match theme.as_deref() {
            Some("\"dark\"") => Theme::change(ThemeMode::Dark, None, cx),
            Some("\"light\"") => Theme::change(ThemeMode::Light, None, cx),
            _ => Theme::sync_system_appearance(None, cx),
        }

        let bounds = Bounds::centered(None, size(px(1400.), px(900.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(800.), px(500.))),
            ..WindowOptions::default()
        };
        let (handle, ws) = gpui_kit::open_window(options, cx, |window, cx| {
            window.set_window_title("Forkline");
            cx.new(|cx| Workspace::new(!bench, window, cx))
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
