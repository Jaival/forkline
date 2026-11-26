//! Shared setup for UI tests: a workspace window over a temp database.

use gpui_kit::{AnyWindowHandle, AppContext as _, Bounds, Entity, TestAppContext, WindowBounds, WindowOptions, px, size};

use gpui_kit::test::TestWindowExt as _;

use crate::store::Store;
use crate::workspace::Workspace;
use crate::{panel, signin, views, workspace};

pub fn open(cx: &mut TestAppContext, dir: &std::path::Path) -> (AnyWindowHandle, Entity<Workspace>) {
    open_seeded(cx, dir, "")
}

/// A GitHub API base nothing listens on: tests never reach GitHub.
pub const NO_GITHUB: &str = "http://127.0.0.1:9";

/// Run `sql` on the database before the window opens. Signed in (a test token).
pub fn open_seeded(
    cx: &mut TestAppContext,
    dir: &std::path::Path,
    sql: &str,
) -> (AnyWindowHandle, Entity<Workspace>) {
    open_with(cx, dir, sql, NO_GITHUB, Some("test-token"))
}

/// Like [`open_seeded`] with the engine on `api` (a mock GitHub) and
/// `token` in memory; `None` starts signed out.
pub fn open_with(
    cx: &mut TestAppContext,
    dir: &std::path::Path,
    sql: &str,
    api: &str,
    token: Option<&str>,
) -> (AnyWindowHandle, Entity<Workspace>) {
    let dir = dir.to_owned();
    // The footer awaits engine calls on the real tokio runtime; their
    // wake-ups come from other threads, which the test scheduler rejects.
    cx.executor().allow_parking();
    cx.update(|cx| {
        gpui_kit::init(cx);
        // End states at once; tests in `motion.rs` turn motion back on.
        cx.set_reduce_motion(true);
        crate::theme::apply(Some(gpui_kit::component::ThemeMode::Dark), None, cx);
        panel::init(cx);
        workspace::init(cx);
        views::init(cx);
        signin::init(cx);
        crate::appearance::init(cx);
        crate::drafts::Drafts::init(cx);
        Store::init(dir, fl_sync::Config::test(api), true, false, cx).unwrap();
        let store = Store::global(cx);
        store.read(cx).sync().set_test_token(token);
        store.read(cx).db().write(|c| c.execute_batch(sql)).unwrap();
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(1200.), px(800.))))),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Workspace::new(true, window, cx))).unwrap()
    })
}

/// Run the app until `done` holds; engine calls finish on other threads.
pub fn wait_for(cx: &mut TestAppContext, what: &str, done: impl Fn(&mut TestAppContext) -> bool) {
    let start = std::time::Instant::now();
    loop {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(2), "timed out waiting for: {what}");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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

/// Run `f` in a test app whose text system is the real one, so layout uses
/// the fonts' real metrics.
pub fn with_real_text(name: &'static str, f: impl FnOnce(&mut TestAppContext) + 'static) {
    let f = std::panic::AssertUnwindSafe(f);
    gpui_kit::run_test_once(
        0,
        Box::new(move |dispatcher| {
            // Move the whole wrapper in, not just its field.
            let f = f;
            let ts = gpui_kit::platform::current_platform(false).text_system();
            let mut cx = TestAppContext::build_with_text_system(dispatcher, Some(name), ts);
            (f.0)(&mut cx);
            cx.run_until_parked();
            cx.update(|cx| {
                cx.background_executor().forbid_parking();
                cx.quit();
            });
            cx.run_until_parked();
        }),
    );
}
