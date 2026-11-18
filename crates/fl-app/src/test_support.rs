//! Shared setup for UI tests: a workspace window over a temp database.

use gpui_kit::{AnyWindowHandle, AppContext as _, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions, px, size};

use gpui_kit::test::TestWindowExt as _;

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

/// Open the workspace over `sql`, open `view` and draw a frame.
pub fn start_on(
    cx: &mut TestAppContext,
    dir: &std::path::Path,
    sql: &str,
    view: crate::view_ref::ViewRef,
) -> (AnyWindowHandle, Entity<Workspace>) {
    let (handle, ws) = open_seeded(cx, dir, sql);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open(view, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    (handle, ws)
}

/// The focused tab's content view as `T`.
pub fn active<T: 'static>(cx: &mut TestAppContext, handle: AnyWindowHandle, ws: &Entity<Workspace>) -> Entity<T> {
    cx.update_window(handle, |_, window, cx| ws.read(cx).active_view(window, cx).unwrap().downcast::<T>().ok().unwrap())
        .unwrap()
}

/// Press `key`, draw a frame, and let background reads finish.
pub fn press(cx: &mut TestAppContext, handle: AnyWindowHandle, key: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.press(key, cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

/// Type `text` into the focused input.
pub fn type_text(cx: &mut TestAppContext, handle: AnyWindowHandle, text: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.input(text, cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

/// Tab titles of the first tab group; the active one starts with `*`.
pub fn tabs(cx: &mut TestAppContext, ws: &Entity<Workspace>) -> Vec<String> {
    cx.read(|cx| ws.read(cx).tab_groups(cx).into_iter().next().unwrap())
}

/// Run `sql` on the test database and tell the views `scopes` changed.
pub fn write(cx: &mut TestAppContext, sql: &str, scopes: &[&str]) {
    cx.update(|cx| {
        Store::global(cx).read(cx).db().write(|c| c.execute_batch(sql)).unwrap();
        Store::emit_changed(scopes, cx);
    });
    cx.run_until_parked();
}

/// One integer from the test database.
pub fn query_i64(cx: &mut TestAppContext, sql: &str) -> i64 {
    cx.read(|cx| Store::global(cx).read(cx).db().read(|c| c.query_row(sql, [], |r| r.get(0))).unwrap())
}
