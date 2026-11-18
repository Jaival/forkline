//! Shared setup for UI tests: a workspace window over a temp database.

use gpui_kit::{AnyWindowHandle, AppContext as _, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions, px, size};

use crate::store::Store;
use crate::workspace::Workspace;
use crate::{panel, views, workspace};

pub fn open(cx: &mut TestAppContext, dir: &std::path::Path) -> (AnyWindowHandle, Entity<Workspace>) {
    open_seeded(cx, dir, "")
}

/// Run `sql` on the database before the window opens.
pub fn open_seeded(
    cx: &mut TestAppContext,
    dir: &std::path::Path,
    sql: &str,
) -> (AnyWindowHandle, Entity<Workspace>) {
    let dir = dir.to_owned();
    // The footer awaits engine calls on the real tokio runtime; their
    // wake-ups come from other threads, which the test scheduler rejects.
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        panel::init(cx);
        workspace::init(cx);
        views::init(cx);
        Store::init(dir, true, false, cx).unwrap();
        Store::global(cx).read(cx).db().write(|c| c.execute_batch(sql)).unwrap();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(1200.), px(800.))))),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Workspace::new(true, window, cx))).unwrap()
    })
}
