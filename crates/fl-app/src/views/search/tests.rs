use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};

use super::*;
use crate::test_support::{active, open_seeded, press, tabs, type_text};
use crate::workspace::Workspace;

const SEED: &str = "
    INSERT INTO users(id, login) VALUES (1, 'me');
    INSERT INTO repos(id, node_id, owner, name, updated_at) VALUES (1,'R','o','a',0);
    INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,body_md,author_id,created_at,updated_at) VALUES
      (1,'I',1,1,0,'open','pager cursor bug','the pager skips a page',1,0,10),
      (2,'I',1,2,1,'open','fix the pager','',1,0,30),
      (3,'I',1,3,0,'open','unrelated','nothing here',1,0,20);
    INSERT INTO comments(id, issue_id, author_id, body_md, created_at, updated_at) VALUES
      (9, 3, 1, 'mentions the pager once', 0, 0);";

/// Index the seed, then open a Search tab for `q`.
fn start(cx: &mut TestAppContext, dir: &std::path::Path, q: &str) -> (AnyWindowHandle, Entity<Workspace>, Entity<SearchView>) {
    let (handle, ws) = open_seeded(cx, dir, SEED);
    cx.update(|cx| {
        Store::global(cx)
            .read(cx)
            .db()
            .write(|c| (1..=3).try_for_each(|id| fl_search::index_issue(c, id)))
            .unwrap();
    });
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open(ViewRef::Search { q: q.into() }, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    let view = active::<SearchView>(cx, handle, &ws);
    (handle, ws, view)
}

fn ids(cx: &mut TestAppContext, view: &Entity<SearchView>) -> Vec<i64> {
    cx.read(|cx| view.read(cx).ids())
}

/// `(input focused, list focused)`.
fn focus(cx: &mut TestAppContext, handle: AnyWindowHandle, view: &Entity<SearchView>) -> (bool, bool) {
    cx.update_window(handle, |_, window, cx| {
        let v = view.read(cx);
        (v.input().focus_handle(cx).is_focused(window), v.focus.is_focused(window))
    })
    .unwrap()
}

#[test]
fn split_snippet_strips_marks_and_keeps_byte_ranges() {
    let (text, hits) = split_snippet("a \u{E000}pager\u{E001} é \u{E000}x\u{E001}");
    assert_eq!(text, "a pager é x");
    assert_eq!(hits, [2..7, 11..12]);
    assert_eq!(&text[hits[1].clone()], "x");
    // An unclosed or empty mark adds no range.
    assert_eq!(split_snippet("\u{E000}\u{E001}a\u{E000}b").1, Vec::<Range<usize>>::new());
}

#[gpui_kit::test]
fn a_query_lists_its_hits(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _, view) = start(cx, dir.path(), "pager");
    let mut found = ids(cx, &view);
    found.sort();
    assert_eq!(found, [1, 2, 3]);
    cx.read(|cx| {
        let v = view.read(cx);
        assert!(v.status().starts_with("3 results"), "{}", v.status());
        let in_comment = v.rows.items.iter().find(|h| h.row.id == 3).unwrap();
        assert_eq!(in_comment.comment_id, Some(9));
    });
    assert_eq!(focus(cx, handle, &view), (false, true));
}

#[gpui_kit::test]
fn an_empty_query_shows_help_and_focuses_the_bar(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _, view) = start(cx, dir.path(), "");
    assert!(ids(cx, &view).is_empty());
    assert_eq!(focus(cx, handle, &view), (true, false));
}

#[gpui_kit::test]
fn enter_in_the_bar_runs_the_query_in_this_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws, view) = start(cx, dir.path(), "");
    type_text(cx, handle, "unrelated");
    press(cx, handle, "enter");
    assert_eq!(ids(cx, &view), [3]);
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Search: unrelated"]);
    assert_eq!(focus(cx, handle, &view), (false, true));
}

#[gpui_kit::test]
fn j_and_enter_open_the_hit(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws, _) = start(cx, dir.path(), "fix");
    press(cx, handle, "j");
    press(cx, handle, "enter");
    assert_eq!(tabs(cx, &ws), ["Inbox", "Search: fix", "*#2 fix the pager"]);
}

#[gpui_kit::test]
fn slash_focuses_the_bar_and_escape_leaves_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _, view) = start(cx, dir.path(), "pager");
    press(cx, handle, "/");
    assert_eq!(focus(cx, handle, &view), (true, false));
    // `j` types in the bar, it does not move the selection.
    type_text(cx, handle, "j");
    cx.read(|cx| assert_eq!(view.read(cx).rows.selected, None));
    press(cx, handle, "escape");
    assert_eq!(focus(cx, handle, &view), (false, true));
    // The typed text did not run.
    assert_eq!(ids(cx, &view).len(), 3);
}

#[gpui_kit::test]
fn no_hits_says_so(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, _, view) = start(cx, dir.path(), "zzzznomatch");
    assert!(ids(cx, &view).is_empty());
    cx.read(|cx| assert_eq!(view.read(cx).status(), "No results."));
}

#[gpui_kit::test]
fn the_rail_selects_the_open_search_tab_with_its_query_selected(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws, view) = start(cx, dir.path(), "fix");
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open(ViewRef::Inbox, window, cx));
        window.render_frame(cx);
        window.click("rail-search", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Search: fix"]);
    // The query is selected: typing replaces it.
    type_text(cx, handle, "pager");
    cx.read(|cx| assert_eq!(view.read(cx).input.read(cx).value(), "pager"));
}
