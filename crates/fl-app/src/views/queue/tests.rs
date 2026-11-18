use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, Entity, Focusable as _, TestAppContext};

use super::*;
use crate::test_support::open_seeded;
use crate::view_ref::ViewRef;
use crate::views::list_view::ListView;
use crate::workspace::Workspace;

const SEED: &str = "
    INSERT INTO users(id, login) VALUES (1, 'me'), (2, 'bob');
    INSERT INTO repos(id, node_id, owner, name, updated_at) VALUES (1,'R','o','a',0), (2,'R','o','b',0);
    INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,author_id,created_at,updated_at) VALUES
      (1,'I',1,1,0,'open','an issue',1,0,10),
      (3,'I',2,1,1,'open','a pull request',2,0,30);
    INSERT INTO notifications(id,repo_id,subject_type,issue_id,title,reason,unread,done,updated_at) VALUES
      ('rev',2,'PullRequest',3,'t','review_requested',1,0,50),
      ('men1',1,'Issue',1,'t','mention',1,0,90),
      ('men2',1,'Issue',1,'t','mention',0,0,80),
      ('sub',1,'Issue',1,'t','subscribed',1,0,100),
      ('rel',2,'Release',NULL,'t','subscribed',1,0,10);";

fn row(key: &str, group: QueueGroup) -> QueueRow {
    QueueRow {
        key: key.into(),
        group,
        issue_id: None,
        repo_id: 1,
        repo: "o/a".into(),
        number: None,
        is_pr: false,
        state: None,
        title: String::new(),
        author: None,
        checks: None,
        updated_at: 0,
        unread: false,
    }
}

/// Open the workspace over `sql`, with the first frame drawn.
fn start(cx: &mut TestAppContext, dir: &std::path::Path, sql: &str) -> (AnyWindowHandle, Entity<Workspace>) {
    let (handle, ws) = open_seeded(cx, dir, sql);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    (handle, ws)
}

fn list(cx: &mut TestAppContext, handle: AnyWindowHandle, ws: &Entity<Workspace>) -> Entity<ListView> {
    cx.update_window(handle, |_, window, cx| {
        ws.read(cx).active_view(window, cx).unwrap().downcast::<ListView>().ok().unwrap()
    })
    .unwrap()
}

fn queue_of(cx: &mut TestAppContext, handle: AnyWindowHandle, ws: &Entity<Workspace>) -> Entity<Queue> {
    let list = list(cx, handle, ws);
    cx.read(|cx| list.read(cx).queue.clone())
}

fn press(cx: &mut TestAppContext, handle: AnyWindowHandle, key: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.press(key, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

fn selected(cx: &mut TestAppContext, queue: &Entity<Queue>) -> Option<String> {
    cx.read(|cx| queue.read(cx).selected_key().map(str::to_owned))
}

fn tabs(cx: &mut TestAppContext, ws: &Entity<Workspace>) -> Vec<String> {
    cx.read(|cx| ws.read(cx).tab_groups(cx).into_iter().next().unwrap())
}

#[test]
fn flatten_puts_one_header_per_group() {
    use QueueGroup::*;
    let rows = [row("a", ReviewRequested), row("b", ReviewRequested), row("c", Mentioned), row("d", Other)];
    assert_eq!(
        flatten(&rows, false),
        [
            Item::Header(ReviewRequested),
            Item::Row(0),
            Item::Row(1),
            Item::Header(Mentioned),
            Item::Row(2),
            Item::Header(Other),
            Item::Row(3),
        ]
    );
    assert!(flatten(&[], false).is_empty());
}

#[test]
fn age_units() {
    let now = 100_000_000;
    let ago = |s: i64| age(now, now - s);
    assert_eq!(ago(-5), "now");
    assert_eq!(ago(59), "now");
    assert_eq!(ago(300), "5m");
    assert_eq!(ago(2 * 3_600), "2h");
    assert_eq!(ago(3 * 86_400), "3d");
    assert_eq!(ago(4 * 604_800), "4w");
    assert_eq!(ago(2 * 31_536_000), "2y");
}

#[gpui_kit::test]
fn inbox_shows_seeded_rows_in_group_order(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    let queue = queue_of(cx, handle, &ws);
    cx.read(|cx| assert_eq!(queue.read(cx).keys(), ["rev", "men1", "men2", "sub", "rel"]));
}

#[gpui_kit::test]
fn j_and_k_move_the_selection(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    let queue = queue_of(cx, handle, &ws);
    assert_eq!(selected(cx, &queue), None);
    press(cx, handle, "j");
    press(cx, handle, "j");
    press(cx, handle, "k");
    assert_eq!(selected(cx, &queue).as_deref(), Some("rev"));
    press(cx, handle, "k");
    assert_eq!(selected(cx, &queue).as_deref(), Some("rev"));
    // `rev` ends the first group; `men1` starts the next.
    press(cx, handle, "down");
    assert_eq!(selected(cx, &queue).as_deref(), Some("men1"));
    press(cx, handle, "up");
    assert_eq!(selected(cx, &queue).as_deref(), Some("rev"));
}

#[gpui_kit::test]
fn k_with_no_selection_selects_the_first_row(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    let queue = queue_of(cx, handle, &ws);
    press(cx, handle, "k");
    assert_eq!(selected(cx, &queue).as_deref(), Some("rev"));
}

#[gpui_kit::test]
fn enter_opens_a_pull_request(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    press(cx, handle, "j");
    press(cx, handle, "enter");
    cx.run_until_parked();
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Pull request 3"]);
}

#[gpui_kit::test]
fn enter_opens_an_issue(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    press(cx, handle, "j");
    press(cx, handle, "j");
    press(cx, handle, "enter");
    cx.run_until_parked();
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Issue 1"]);
}

#[gpui_kit::test]
fn enter_on_a_row_without_an_item_does_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    for _ in 0..5 {
        press(cx, handle, "j");
    }
    press(cx, handle, "enter");
    cx.run_until_parked();
    assert_eq!(tabs(cx, &ws), ["*Inbox"]);
}

#[gpui_kit::test]
fn click_selects_and_double_click_opens(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    let queue = queue_of(cx, handle, &ws);
    cx.update_window(handle, |_, window, cx| window.click(("queue-row", 1usize), cx)).unwrap();
    assert_eq!(selected(cx, &queue).as_deref(), Some("men1"));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.double_click(("queue-row", 0usize), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Pull request 3"]);
}

#[gpui_kit::test]
fn selection_shows_in_the_reading_pane(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    let list = list(cx, handle, &ws);
    let reading = cx.read(|cx| list.read(cx).reading.clone());
    cx.read(|cx| assert!(reading.read(cx).item.is_none()));
    press(cx, handle, "j");
    press(cx, handle, "j");
    cx.read(|cx| assert_eq!(reading.read(cx).item.as_ref().map(|r| r.key.as_str()), Some("men1")));
}

#[gpui_kit::test]
fn pulls_load_a_second_page(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let sql = "
        INSERT INTO repos(id, node_id, owner, name, updated_at) VALUES (1,'R','o','a',0);
        WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 150)
        INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,created_at,updated_at)
          SELECT i,'I',1,i,1,'open','pr',0,i FROM n;";
    let (handle, ws) = start(cx, dir.path(), sql);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open(ViewRef::Pulls { repo_id: None }, window, cx));
    })
    .unwrap();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    let queue = queue_of(cx, handle, &ws);
    cx.read(|cx| assert_eq!(queue.read(cx).rows.len(), 100));
    queue.update(cx, |q, cx| q.load_more(cx));
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(queue.read(cx).rows.len(), 150);
        assert_eq!(queue.read(cx).next, None);
    });
}

#[gpui_kit::test]
fn reload_keeps_the_selection_by_key(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    let queue = queue_of(cx, handle, &ws);
    press(cx, handle, "j");
    press(cx, handle, "j");
    assert_eq!(selected(cx, &queue).as_deref(), Some("men1"));
    cx.update(|cx| {
        Store::global(cx)
            .read(cx)
            .db()
            .write(|c| {
                c.execute(
                    "INSERT INTO notifications(id,repo_id,subject_type,issue_id,title,reason,unread,done,updated_at) \
                     VALUES ('new',2,'PullRequest',3,'t','review_requested',1,0,500)",
                    [],
                )
            })
            .unwrap();
    });
    queue.update(cx, |q, cx| q.reload(cx));
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(queue.read(cx).keys(), ["new", "rev", "men1", "men2", "sub", "rel"]);
        assert_eq!(queue.read(cx).selected_key(), Some("men1"));
    });
}

#[gpui_kit::test]
fn repo_filter_keeps_that_repos_rows(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    let queue = queue_of(cx, handle, &ws);
    press(cx, handle, "j");
    queue.update(cx, |q, cx| q.set_repo(Some(1), cx));
    cx.read(|cx| {
        assert_eq!(queue.read(cx).keys(), ["men1", "men2", "sub"]);
        assert_eq!(queue.read(cx).selected_key(), None);
    });
}

#[gpui_kit::test]
fn picking_a_repo_names_the_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open(ViewRef::Pulls { repo_id: None }, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    let queue = queue_of(cx, handle, &ws);
    cx.update_window(handle, |_, window, cx| queue.update(cx, |q, cx| q.pick_repo(Some(2), window, cx))).unwrap();
    cx.run_until_parked();
    assert_eq!(tabs(cx, &ws), ["Inbox", "*o/b: Pull requests"]);
    cx.update_window(handle, |_, window, cx| queue.update(cx, |q, cx| q.pick_repo(None, window, cx))).unwrap();
    cx.run_until_parked();
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Pull requests"]);
}

/// Index the seeded issues, so the `/` filter finds their words.
fn index(cx: &mut TestAppContext) {
    cx.update(|cx| {
        Store::global(cx)
            .read(cx)
            .db()
            .write(|c| {
                fl_search::index_issue(c, 1)?;
                fl_search::index_issue(c, 3)
            })
            .unwrap();
    });
}

fn type_text(cx: &mut TestAppContext, handle: AnyWindowHandle, text: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.input(text, cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

/// `(input focused, queue focused)`.
fn focus_of(cx: &mut TestAppContext, handle: AnyWindowHandle, queue: &Entity<Queue>) -> (bool, bool) {
    cx.update_window(handle, |_, window, cx| {
        let queue = queue.read(cx);
        (queue.input().focus_handle(cx).is_focused(window), queue.focus.is_focused(window))
    })
    .unwrap()
}

fn focus_queue(cx: &mut TestAppContext, handle: AnyWindowHandle, queue: &Entity<Queue>) {
    cx.update_window(handle, |_, window, cx| window.focus(&queue.read(cx).focus.clone(), cx)).unwrap();
}

#[gpui_kit::test]
fn slash_focuses_the_query_bar(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    let queue = queue_of(cx, handle, &ws);
    focus_queue(cx, handle, &queue);
    press(cx, handle, "/");
    assert_eq!(focus_of(cx, handle, &queue), (true, false));
    // Typing `j` goes to the input, not to the list.
    type_text(cx, handle, "j");
    assert_eq!(selected(cx, &queue), None);
    cx.read(|cx| assert_eq!(queue.read(cx).filter(), "j"));
}

#[gpui_kit::test]
fn typing_filters_the_rows_and_clears_the_selection(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    index(cx);
    let queue = queue_of(cx, handle, &ws);
    press(cx, handle, "j");
    assert_eq!(selected(cx, &queue).as_deref(), Some("rev"));
    press(cx, handle, "/");
    type_text(cx, handle, "pull");
    cx.read(|cx| assert_eq!(queue.read(cx).keys(), ["rev"]));
    assert_eq!(selected(cx, &queue), None);
    cx.read(|cx| {
        assert!(queue.read(cx).counts_ready);
        assert!(queue.read(cx).filter_ms.is_some());
    });
    // The first `j` after the filter selects the first filtered row.
    press(cx, handle, "enter");
    press(cx, handle, "j");
    assert_eq!(selected(cx, &queue).as_deref(), Some("rev"));
}

#[gpui_kit::test]
fn escape_clears_the_filter_and_returns_the_focus(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    index(cx);
    let queue = queue_of(cx, handle, &ws);
    press(cx, handle, "/");
    type_text(cx, handle, "pull");
    cx.read(|cx| assert_eq!(queue.read(cx).keys(), ["rev"]));
    press(cx, handle, "escape");
    cx.run_until_parked();
    assert_eq!(focus_of(cx, handle, &queue), (false, true));
    cx.read(|cx| {
        assert_eq!(queue.read(cx).filter(), "");
        assert_eq!(queue.read(cx).input().read(cx).value(), "");
        assert_eq!(queue.read(cx).keys(), ["rev", "men1", "men2", "sub", "rel"]);
    });
    press(cx, handle, "j");
    assert_eq!(selected(cx, &queue).as_deref(), Some("rev"));
}

#[gpui_kit::test]
fn enter_keeps_the_filter_and_returns_the_focus(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    index(cx);
    let queue = queue_of(cx, handle, &ws);
    press(cx, handle, "/");
    type_text(cx, handle, "issue");
    press(cx, handle, "enter");
    cx.run_until_parked();
    assert_eq!(focus_of(cx, handle, &queue), (false, true));
    cx.read(|cx| {
        assert_eq!(queue.read(cx).filter(), "issue");
        assert_eq!(queue.read(cx).keys(), ["men1", "men2", "sub"]);
    });
    press(cx, handle, "j");
    assert_eq!(selected(cx, &queue).as_deref(), Some("men1"));
    // The key `enter` of the list did not run: no tab opened.
    assert_eq!(tabs(cx, &ws), ["*Inbox"]);
}

#[gpui_kit::test]
fn slash_from_the_reading_pane_reaches_the_queue(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    let queue = queue_of(cx, handle, &ws);
    let list = list(cx, handle, &ws);
    let reading = cx.read(|cx| list.read(cx).reading.clone());
    cx.update_window(handle, |_, window, cx| window.focus(&reading.read(cx).focus.clone(), cx)).unwrap();
    press(cx, handle, "/");
    assert_eq!(focus_of(cx, handle, &queue), (true, false));
}

#[gpui_kit::test]
fn save_view_writes_a_row_and_the_queue_lists_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    index(cx);
    let queue = queue_of(cx, handle, &ws);
    // An empty filter saves nothing.
    cx.update_window(handle, |_, window, cx| window.dispatch_action(Box::new(SaveView), cx)).unwrap();
    cx.run_until_parked();
    cx.read(|cx| assert!(queue.read(cx).view_queries().is_empty()));
    press(cx, handle, "/");
    type_text(cx, handle, "issue");
    cx.update_window(handle, |_, window, cx| window.dispatch_action(Box::new(SaveView), cx)).unwrap();
    cx.run_until_parked();
    let saved = cx.update(|cx| Store::global(cx).read(cx).db().read(views::list).unwrap());
    assert_eq!(saved.len(), 1);
    assert_eq!((saved[0].name.as_str(), saved[0].query.as_str(), saved[0].kind), ("issue", "issue", QueueKind::Inbox));
    cx.read(|cx| assert_eq!(queue.read(cx).view_queries(), ["issue"]));
}

#[gpui_kit::test]
fn set_filter_filters_a_list_view(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    index(cx);
    let queue = queue_of(cx, handle, &ws);
    let list = list(cx, handle, &ws);
    let view: gpui_kit::AnyView = list.clone().into();
    let done = cx.update_window(handle, |_, window, cx| crate::views::set_filter(&view, "pull", window, cx)).unwrap();
    assert!(done);
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(queue.read(cx).keys(), ["rev"]);
        assert_eq!(queue.read(cx).input().read(cx).value(), "pull");
    });
    let other: gpui_kit::AnyView = cx.read(|cx| list.read(cx).reading.clone()).into();
    let done = cx.update_window(handle, |_, window, cx| crate::views::set_filter(&other, "x", window, cx)).unwrap();
    assert!(!done);
}

#[gpui_kit::test]
fn a_data_change_reload_keeps_the_filter(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    index(cx);
    let queue = queue_of(cx, handle, &ws);
    let view: gpui_kit::AnyView = list(cx, handle, &ws).into();
    cx.update_window(handle, |_, window, cx| crate::views::set_filter(&view, "issue", window, cx)).unwrap();
    cx.run_until_parked();
    cx.update(|cx| Store::emit_changed(&["inbox"], cx));
    cx.run_until_parked();
    cx.read(|cx| {
        assert_eq!(queue.read(cx).filter(), "issue");
        assert_eq!(queue.read(cx).keys(), ["men1", "men2", "sub"]);
        assert!(queue.read(cx).counts_ready);
    });
}

#[test]
fn toggle_term_adds_and_removes_one_word() {
    assert_eq!(toggle_term("", UNREAD), "is:unread");
    assert_eq!(toggle_term("bug ", UNREAD), "bug is:unread");
    assert_eq!(toggle_term("bug is:unread label:x", UNREAD), "bug label:x");
    assert_eq!(toggle_term("IS:UNREAD", UNREAD), "");
    // `-is:unread` is another word: the button adds the positive one.
    assert_eq!(toggle_term("-is:unread", UNREAD), "-is:unread is:unread");
}

#[gpui_kit::test]
fn the_unread_button_keeps_unread_notifications(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    let queue = queue_of(cx, handle, &ws);
    let click = |cx: &mut TestAppContext| {
        cx.update_window(handle, |_, window, cx| {
            window.click("queue-unread", cx);
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
    };
    click(cx);
    cx.read(|cx| {
        assert_eq!(queue.read(cx).filter(), "is:unread");
        assert_eq!(queue.read(cx).input().read(cx).value(), "is:unread");
        // `men2` is read; `rel` has no cached issue but its flag still counts.
        assert_eq!(queue.read(cx).keys(), ["rev", "men1", "sub", "rel"]);
    });
    click(cx);
    cx.read(|cx| {
        assert_eq!(queue.read(cx).filter(), "");
        assert_eq!(queue.read(cx).keys(), ["rev", "men1", "men2", "sub", "rel"]);
    });
}

#[gpui_kit::test]
fn g_and_the_grouped_button_switch_to_one_list(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    let queue = queue_of(cx, handle, &ws);
    cx.read(|cx| {
        assert!(!queue.read(cx).flat());
        assert_eq!(queue.read(cx).keys(), ["rev", "men1", "men2", "sub", "rel"]);
        assert_eq!(queue.read(cx).headers(), 3);
    });
    press(cx, handle, "g");
    cx.run_until_parked();
    cx.read(|cx| {
        assert!(queue.read(cx).flat());
        // Newest first, no group headers.
        assert_eq!(queue.read(cx).keys(), ["sub", "men1", "men2", "rev", "rel"]);
        assert_eq!(queue.read(cx).headers(), 0);
    });
    cx.update_window(handle, |_, window, cx| {
        window.click("queue-grouped", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.read(|cx| {
        assert!(!queue.read(cx).flat());
        assert_eq!(queue.read(cx).keys(), ["rev", "men1", "men2", "sub", "rel"]);
    });
}

#[gpui_kit::test]
fn g_works_from_the_reading_pane_and_keeps_the_filter(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path(), SEED);
    let queue = queue_of(cx, handle, &ws);
    let list = list(cx, handle, &ws);
    cx.update_window(handle, |_, window, cx| {
        queue.update(cx, |q, cx| q.set_filter_text("is:unread", window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    // Focus in the reading pane: the list view forwards `g` to the queue.
    cx.update_window(handle, |_, window, cx| {
        let focus = list.read(cx).reading.read(cx).focus.clone();
        window.focus(&focus, cx);
        window.render_frame(cx);
    })
    .unwrap();
    press(cx, handle, "g");
    cx.run_until_parked();
    cx.read(|cx| {
        assert!(queue.read(cx).flat());
        assert_eq!(queue.read(cx).filter(), "is:unread");
        assert_eq!(queue.read(cx).keys(), ["sub", "men1", "rev", "rel"]);
    });
}
