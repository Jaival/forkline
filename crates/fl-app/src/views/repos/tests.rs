use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Focusable as _, TestAppContext};

use super::*;
use crate::test_support::{active, press, query_i64, start_on, tabs, type_text, write};

const SEED: &str = "
    INSERT INTO repos(id, node_id, owner, name, description, language, stars, starred, pinned, explore_only, updated_at) VALUES
      (1,'R1','o','beta','second',  'Rust', 10, 1, 0, 0, 0),
      (2,'R2','o','alpha',NULL,     'Go',    5, 0, 1, 0, 0),
      (3,'R3','x','gamma','third',  NULL,    1, 1, 0, 0, 0),
      (4,'R4','z','explored','only from explore', NULL, 1, 0, 0, 1, 0);";

type Opened = (tempfile::TempDir, gpui_kit::AnyWindowHandle, Entity<crate::workspace::Workspace>, Entity<ReposView>);

fn open(cx: &mut TestAppContext, view: ViewRef) -> Opened {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), SEED, view);
    let view = active::<ReposView>(cx, handle, &ws);
    (dir, handle, ws, view)
}

fn names(cx: &mut TestAppContext, view: &Entity<ReposView>) -> Vec<String> {
    cx.read(|cx| view.read(cx).names())
}

fn selected(cx: &mut TestAppContext, view: &Entity<ReposView>) -> Option<i64> {
    cx.read(|cx| view.read(cx).rows.selected_row().map(|r| r.id))
}

#[gpui_kit::test]
fn repositories_list_every_repo_but_explore_only_ones(cx: &mut TestAppContext) {
    let (_d, _, _, view) = open(cx, ViewRef::Repos);
    assert_eq!(names(cx, &view), ["o/alpha", "o/beta", "x/gamma"]);
}

#[gpui_kit::test]
fn starred_lists_starred_repos_only(cx: &mut TestAppContext) {
    let (_d, _, _, view) = open(cx, ViewRef::Starred);
    assert_eq!(names(cx, &view), ["o/beta", "x/gamma"]);
}

#[gpui_kit::test]
fn j_k_move_from_no_selection(cx: &mut TestAppContext) {
    let (_d, handle, _, view) = open(cx, ViewRef::Repos);
    assert_eq!(selected(cx, &view), None);
    press(cx, handle, "j");
    assert_eq!(selected(cx, &view), Some(2));
    press(cx, handle, "j");
    press(cx, handle, "j");
    press(cx, handle, "j");
    assert_eq!(selected(cx, &view), Some(3));
    press(cx, handle, "k");
    assert_eq!(selected(cx, &view), Some(1));
}

#[gpui_kit::test]
fn enter_opens_the_repo(cx: &mut TestAppContext) {
    let (_d, handle, ws, _) = open(cx, ViewRef::Repos);
    press(cx, handle, "j");
    press(cx, handle, "enter");
    assert_eq!(tabs(cx, &ws), ["Inbox", "Repositories", "*o/alpha"]);
}

#[gpui_kit::test]
fn double_click_opens_the_repo(cx: &mut TestAppContext) {
    let (_d, handle, ws, view) = open(cx, ViewRef::Repos);
    cx.update_window(handle, |_, window, cx| window.click(("repos-row", 1usize), cx)).unwrap();
    assert_eq!(selected(cx, &view), Some(1));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.double_click(("repos-row", 1usize), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(tabs(cx, &ws), ["Inbox", "Repositories", "*o/beta"]);
}

#[gpui_kit::test]
fn p_pins_and_unpins_the_selected_repo(cx: &mut TestAppContext) {
    let (_d, handle, _, view) = open(cx, ViewRef::Repos);
    press(cx, handle, "j");
    press(cx, handle, "j");
    press(cx, handle, "p");
    assert_eq!(query_i64(cx, "SELECT pinned FROM repos WHERE id = 1"), 1);
    // The reload after the write keeps the selection and shows the pin.
    assert_eq!(selected(cx, &view), Some(1));
    cx.read(|cx| assert!(view.read(cx).rows.selected_row().unwrap().pinned));
    press(cx, handle, "p");
    assert_eq!(query_i64(cx, "SELECT pinned FROM repos WHERE id = 1"), 0);
}

#[gpui_kit::test]
fn pin_button_pins_its_row(cx: &mut TestAppContext) {
    let (_d, handle, _, _) = open(cx, ViewRef::Repos);
    cx.update_window(handle, |_, window, cx| window.click(("repo-pin", 2usize), cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(query_i64(cx, "SELECT pinned FROM repos WHERE id = 3"), 1);
}

#[gpui_kit::test]
fn slash_filters_and_escape_clears(cx: &mut TestAppContext) {
    let (_d, handle, _, view) = open(cx, ViewRef::Repos);
    press(cx, handle, "j");
    press(cx, handle, "/");
    let input_focused = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| view.read(cx).input().focus_handle(cx).is_focused(window)).unwrap()
    };
    assert!(input_focused(cx));
    type_text(cx, handle, "ph");
    assert_eq!(names(cx, &view), ["o/alpha"]);
    assert_eq!(selected(cx, &view), None);
    // `j` types while the input has the focus.
    type_text(cx, handle, "j");
    assert_eq!(names(cx, &view), Vec::<String>::new());
    press(cx, handle, "escape");
    assert!(!input_focused(cx));
    assert_eq!(names(cx, &view), ["o/alpha", "o/beta", "x/gamma"]);
    press(cx, handle, "j");
    assert_eq!(selected(cx, &view), Some(2));
}

#[gpui_kit::test]
fn enter_in_the_filter_keeps_it_and_returns_the_keys(cx: &mut TestAppContext) {
    let (_d, handle, _, view) = open(cx, ViewRef::Repos);
    press(cx, handle, "/");
    type_text(cx, handle, "gam");
    press(cx, handle, "enter");
    press(cx, handle, "j");
    assert_eq!(names(cx, &view), ["x/gamma"]);
    assert_eq!(selected(cx, &view), Some(3));
}

#[gpui_kit::test]
fn reload_on_repos_keeps_the_selection(cx: &mut TestAppContext) {
    let (_d, handle, _, view) = open(cx, ViewRef::Repos);
    press(cx, handle, "j");
    press(cx, handle, "j");
    write(cx, "INSERT INTO repos(id,node_id,owner,name,updated_at) VALUES (5,'R5','a','first',0)", &["repos"]);
    assert_eq!(names(cx, &view), ["a/first", "o/alpha", "o/beta", "x/gamma"]);
    assert_eq!(selected(cx, &view), Some(1));
}

#[gpui_kit::test]
fn empty_starred_view(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), "", ViewRef::Starred);
    let view = active::<ReposView>(cx, handle, &ws);
    assert!(names(cx, &view).is_empty());
    press(cx, handle, "j");
    press(cx, handle, "enter");
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Starred"]);
}
