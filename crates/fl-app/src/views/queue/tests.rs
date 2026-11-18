use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

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
        flatten(&rows),
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
    assert!(flatten(&[]).is_empty());
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
