use gpui_kit::TestAppContext;

use super::*;
use crate::test_support::{active, press, start_on, tabs, write};

const SEED: &str = "
    INSERT INTO users(id, login) VALUES (1, 'me');
    INSERT INTO repos(id, node_id, owner, name, updated_at) VALUES (1,'R','o','a',0);
    INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,author_id,created_at,updated_at) VALUES
      (1,'I',1,1,0,'open','old issue',1,0,10),
      (2,'I',1,2,1,'open','new pr',1,0,30),
      (3,'I',1,3,0,'closed','closed issue',1,0,40),
      (4,'I',1,4,0,'open','middle issue',1,0,20);";

fn ids(cx: &mut TestAppContext, view: &Entity<HomeView>) -> Vec<i64> {
    cx.read(|cx| view.read(cx).ids())
}

fn selected(cx: &mut TestAppContext, view: &Entity<HomeView>) -> Option<i64> {
    cx.read(|cx| view.read(cx).selected_id())
}

use gpui_kit::Entity;

#[gpui_kit::test]
fn open_items_newest_first(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), SEED, ViewRef::Home);
    let view = active::<HomeView>(cx, handle, &ws);
    assert_eq!(ids(cx, &view), [2, 4, 1]);
    assert_eq!(selected(cx, &view), None);
}

#[gpui_kit::test]
fn j_k_and_enter_open_the_item(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), SEED, ViewRef::Home);
    let view = active::<HomeView>(cx, handle, &ws);
    press(cx, handle, "j");
    press(cx, handle, "j");
    press(cx, handle, "k");
    assert_eq!(selected(cx, &view), Some(2));
    press(cx, handle, "enter");
    assert_eq!(tabs(cx, &ws), ["Inbox", "Home", "*Pull request 2"]);
}

#[gpui_kit::test]
fn enter_on_an_issue_opens_an_issue_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), SEED, ViewRef::Home);
    press(cx, handle, "j");
    press(cx, handle, "j");
    press(cx, handle, "enter");
    assert_eq!(tabs(cx, &ws), ["Inbox", "Home", "*Issue 4"]);
}

#[gpui_kit::test]
fn reload_on_issues_keeps_the_selection(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), SEED, ViewRef::Home);
    let view = active::<HomeView>(cx, handle, &ws);
    press(cx, handle, "j");
    press(cx, handle, "j");
    write(cx, "UPDATE issues SET updated_at = 50 WHERE id = 1", &["issues"]);
    assert_eq!(ids(cx, &view), [1, 2, 4]);
    assert_eq!(selected(cx, &view), Some(4));
}

#[gpui_kit::test]
fn no_items_shows_the_empty_state(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), "", ViewRef::Home);
    let view = active::<HomeView>(cx, handle, &ws);
    assert!(ids(cx, &view).is_empty());
    press(cx, handle, "j");
    assert_eq!(selected(cx, &view), None);
}
