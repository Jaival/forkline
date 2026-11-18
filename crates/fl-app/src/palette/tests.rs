use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, FocusHandle, TestAppContext, point, px};

use crate::test_support::open_seeded;
use crate::view_ref::ViewRef;
use crate::workspace::Workspace;

const SEED: &str = "
    INSERT INTO users(id, login) VALUES (1, 'me'), (2, 'bob');
    INSERT INTO repos(id, node_id, owner, name, updated_at) VALUES (1,'R','o','a',0);
    INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,author_id,created_at,updated_at) VALUES
      (1,'I',1,7,0,'open','needle in haystack',1,0,10),
      (3,'I',1,8,1,'open','a pull request',2,0,30);
    INSERT INTO notifications(id,repo_id,subject_type,issue_id,title,reason,unread,done,updated_at) VALUES
      ('11',1,'Issue',1,'t','mention',1,0,90);
    INSERT INTO saved_views(id,name,kind,query,created_at) VALUES (1,'Bugs','pulls','is:open label:bug',0);";

/// Open the workspace over `sql`, with the first frame drawn.
fn start(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Workspace>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = open_seeded(cx, dir.path(), SEED);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        let db = crate::store::Store::global(cx).read(cx).db().clone();
        db.write(|c| fl_search::index_issue(c, 1)).unwrap();
    })
    .unwrap();
    (handle, ws, dir)
}

fn press(cx: &mut TestAppContext, handle: AnyWindowHandle, key: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.press(key, cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn type_text(cx: &mut TestAppContext, handle: AnyWindowHandle, text: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.input(text, cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
}

fn focus_id(cx: &mut TestAppContext, handle: AnyWindowHandle) -> Option<FocusHandle> {
    cx.update_window(handle, |_, window, cx| window.focused(cx)).unwrap()
}

fn labels(cx: &mut TestAppContext, ws: &Entity<Workspace>) -> Option<Vec<String>> {
    cx.read(|cx| ws.read(cx).palette().map(|p| p.read(cx).labels(cx)))
}

fn tabs(cx: &mut TestAppContext, ws: &Entity<Workspace>) -> Vec<String> {
    cx.read(|cx| ws.read(cx).tab_groups(cx).into_iter().next().unwrap())
}

fn has(labels: &Option<Vec<String>>, label: &str) -> bool {
    labels.as_ref().is_some_and(|l| l.iter().any(|x| x == label))
}

#[gpui_kit::test]
fn ctrl_k_opens_focuses_and_toggles(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    let before = focus_id(cx, handle);
    assert!(before.is_some());
    press(cx, handle, "ctrl-k");
    let open = labels(cx, &ws);
    assert!(has(&open, "Go to: Inbox") && has(&open, "New tab"));
    let in_palette = focus_id(cx, handle);
    assert!(in_palette.is_some() && in_palette != before);
    // Ctrl+K again closes it and returns focus.
    press(cx, handle, "ctrl-k");
    assert!(labels(cx, &ws).is_none());
    assert_eq!(focus_id(cx, handle), before);
}

#[gpui_kit::test]
fn typing_filters_the_list(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    press(cx, handle, "ctrl-k");
    type_text(cx, handle, "pull");
    let l = labels(cx, &ws);
    assert!(has(&l, "Go to: Pull requests"));
    assert!(!has(&l, "Go to: Inbox"));
    assert!(has(&l, "Search for \"pull\""));
}

#[gpui_kit::test]
fn enter_runs_the_selected_command(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    press(cx, handle, "ctrl-k");
    type_text(cx, handle, "go to: pull");
    press(cx, handle, "enter");
    assert!(labels(cx, &ws).is_none());
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Pull requests"]);
}

#[gpui_kit::test]
fn escape_clears_then_closes_and_returns_focus(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    let before = focus_id(cx, handle);
    press(cx, handle, "ctrl-k");
    type_text(cx, handle, "zz");
    press(cx, handle, "escape");
    assert!(has(&labels(cx, &ws), "Go to: Inbox"));
    press(cx, handle, "escape");
    assert!(labels(cx, &ws).is_none());
    assert_eq!(focus_id(cx, handle), before);
    // The queue gets keys again.
    press(cx, handle, "j");
}

#[gpui_kit::test]
fn backdrop_click_closes(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    let before = focus_id(cx, handle);
    press(cx, handle, "ctrl-k");
    cx.update_window(handle, |_, window, cx| {
        window.click_at("rail-inbox", point(px(2.), px(2.)), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(labels(cx, &ws).is_none());
    assert_eq!(focus_id(cx, handle), before);
}

#[gpui_kit::test]
fn item_actions_follow_the_view_that_had_focus(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    press(cx, handle, "ctrl-k");
    let l = labels(cx, &ws);
    assert!(has(&l, "Approve") && has(&l, "Filter queue"));
    press(cx, handle, "escape");
    // A placeholder tab has no item actions.
    cx.update_window(handle, |_, window, cx| ws.update(cx, |ws, cx| ws.open(ViewRef::Settings, window, cx))).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    press(cx, handle, "ctrl-k");
    let l = labels(cx, &ws);
    assert!(!has(&l, "Approve") && has(&l, "New tab"));
}

#[gpui_kit::test]
fn search_hit_opens_its_tab(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    press(cx, handle, "ctrl-k");
    type_text(cx, handle, "needle");
    assert!(has(&labels(cx, &ws), "#7 needle in haystack"));
    press(cx, handle, "enter");
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Issue 1"]);
}

#[gpui_kit::test]
fn saved_view_opens_its_list(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    press(cx, handle, "ctrl-k");
    assert!(has(&labels(cx, &ws), "View: Bugs"));
    type_text(cx, handle, "view: bugs");
    press(cx, handle, "enter");
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Pull requests"]);
    let filter = cx
        .update_window(handle, |_, window, cx| {
            let view = ws.read(cx).active_view(window, cx).unwrap();
            crate::views::filter_of(&view, cx)
        })
        .unwrap();
    let query: String = cx.update(|cx| {
        crate::store::Store::global(cx).read(cx).db().read(fl_db::queries::views::list).unwrap()[0].query.clone()
    });
    assert_eq!(filter, Some(query));
}

#[gpui_kit::test]
fn delete_view_removes_the_row(cx: &mut TestAppContext) {
    let (handle, ws, _dir) = start(cx);
    press(cx, handle, "ctrl-k");
    type_text(cx, handle, "delete view: bugs");
    press(cx, handle, "enter");
    cx.run_until_parked();
    let left = cx.update(|cx| {
        crate::store::Store::global(cx).read(cx).db().read(fl_db::queries::views::list).unwrap()
    });
    assert!(left.is_empty());
    press(cx, handle, "ctrl-k");
    assert!(!has(&labels(cx, &ws), "View: Bugs"));
}
