use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusqlite::Connection;

use super::*;
use crate::test_support::open_seeded;
use crate::view_ref::ViewRef;
use crate::views::list_view::ListView;
use crate::views::queue::Queue;
use crate::workspace::Workspace;

// Inbox order: 101 review request (PR 3), 102 mention (issue 1), 103 mention
// (read), 104 subscribed (issue 1), 105 release (no item), 106 PR 5 without
// `pr_details`. Thread ids are numeric, like GitHub's: the action queue parses them.
pub(super) const SEED: &str = "
    INSERT INTO users(id, login) VALUES (1, 'me'), (2, 'bob');
    INSERT INTO repos(id, node_id, owner, name, updated_at) VALUES (1,'R','o','a',0), (2,'R','o','b',0);
    INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,author_id,created_at,updated_at) VALUES
      (1,'I',1,1,0,'open','an issue',1,0,10),
      (3,'I',2,1,1,'open','a pull request',2,0,30),
      (5,'I',2,2,1,'open','a pull request without details',2,0,20);
    INSERT INTO pr_details(issue_id,base_ref,head_ref,base_oid,head_oid,changed_files)
      VALUES (3,'main','topic','b1','h1',1);
    INSERT INTO comments(id,issue_id,author_id,body_md,created_at,updated_at) VALUES
      (1,1,2,'on the issue',1,1),
      (2,3,1,'on the pull request',1,1);
    INSERT INTO commits(repo_id,oid,headline,author_login,authored_at) VALUES (2,'h1','a commit','bob',0);
    INSERT INTO pr_commits(pr_id,oid,position) VALUES (3,'h1',0);
    INSERT INTO checks(id,repo_id,commit_oid,name,status,conclusion) VALUES (1,2,'h1','ci','COMPLETED','SUCCESS');
    INSERT INTO notifications(id,repo_id,subject_type,issue_id,title,reason,unread,done,updated_at) VALUES
      ('101',2,'PullRequest',3,'t','review_requested',1,0,50),
      ('102',1,'Issue',1,'t','mention',1,0,90),
      ('103',1,'Issue',1,'t','mention',0,0,80),
      ('104',1,'Issue',1,'t','subscribed',1,0,100),
      ('105',2,'Release',NULL,'t','subscribed',1,0,10),
      ('106',2,'PullRequest',5,'t','subscribed',1,0,5);";

pub(super) fn start(cx: &mut TestAppContext, dir: &std::path::Path) -> (AnyWindowHandle, Entity<Workspace>) {
    let (handle, ws) = open_seeded(cx, dir, SEED);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    (handle, ws)
}

pub(super) fn list(cx: &mut TestAppContext, handle: AnyWindowHandle, ws: &Entity<Workspace>) -> Entity<ListView> {
    cx.update_window(handle, |_, window, cx| {
        ws.read(cx).active_view(window, cx).unwrap().downcast::<ListView>().ok().unwrap()
    })
    .unwrap()
}

pub(super) fn panes(cx: &mut TestAppContext, handle: AnyWindowHandle, ws: &Entity<Workspace>) -> (Entity<Queue>, Entity<ReadingPane>) {
    let list = list(cx, handle, ws);
    cx.read(|cx| (list.read(cx).queue.clone(), list.read(cx).reading.clone()))
}

pub(super) fn press(cx: &mut TestAppContext, handle: AnyWindowHandle, key: &str) {
    cx.update_window(handle, |_, window, cx| {
        window.press(key, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

/// Press `j` until the queue selects `key`.
pub(super) fn select(cx: &mut TestAppContext, handle: AnyWindowHandle, queue: &Entity<Queue>, key: &str) {
    for _ in 0..10 {
        press(cx, handle, "j");
        if cx.read(|cx| queue.read(cx).selected_key() == Some(key)) {
            return;
        }
    }
    panic!("`j` never selected {key}");
}

/// Park until `done` holds. Work on the tokio runtime ends on another thread,
/// so one park may return before it lands.
pub(super) fn wait_for(cx: &mut TestAppContext, what: &str, done: impl Fn(&mut TestAppContext) -> bool) {
    let start = Instant::now();
    loop {
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(2), "timed out waiting for: {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(super) fn read_db<T>(cx: &mut TestAppContext, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> T {
    cx.update(|cx| Store::global(cx).read(cx).db().read(f).unwrap())
}

fn viewed(cx: &mut TestAppContext, id: i64) -> Option<i64> {
    read_db(cx, |c| c.query_row("SELECT last_viewed_at FROM issues WHERE id = ?1", [id], |r| r.get(0)))
}

fn actions(cx: &mut TestAppContext) -> Vec<(String, String)> {
    read_db(cx, |c| {
        c.prepare("SELECT kind, payload_json FROM action_queue ORDER BY id")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    })
}

pub(super) fn detail_id(cx: &mut TestAppContext, reading: &Entity<ReadingPane>) -> Option<i64> {
    cx.read(|cx| reading.read(cx).detail.as_ref().map(|d| d.row().id))
}

pub(super) fn is_pr_detail(cx: &mut TestAppContext, reading: &Entity<ReadingPane>) -> bool {
    cx.read(|cx| matches!(reading.read(cx).detail, Some(Detail::Pr(_))))
}

fn comment_count(cx: &mut TestAppContext, reading: &Entity<ReadingPane>) -> Option<usize> {
    cx.read(|cx| match &reading.read(cx).detail {
        Some(Detail::Issue { thread, .. }) => Some(thread.comments.len()),
        _ => None,
    })
}

pub(super) fn active_pane(cx: &mut TestAppContext, handle: AnyWindowHandle, ws: &Entity<Workspace>) -> Entity<ReadingPane> {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        ws.read(cx).active_view(window, cx).unwrap().downcast::<ReadingPane>().ok().unwrap()
    })
    .unwrap()
}

#[gpui_kit::test]
fn selecting_a_pull_request_loads_its_detail(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, reading) = panes(cx, handle, &ws);
    select(cx, handle, &queue, "101");
    wait_for(cx, "PR detail", |cx| is_pr_detail(cx, &reading));
    assert_eq!(detail_id(cx, &reading), Some(3));
    cx.read(|cx| assert_eq!(reading.read(cx).tab, PaneTab::Conversation));
}

#[gpui_kit::test]
fn selecting_an_issue_loads_its_comments(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, reading) = panes(cx, handle, &ws);
    select(cx, handle, &queue, "102");
    wait_for(cx, "issue detail", |cx| comment_count(cx, &reading).is_some());
    assert_eq!(detail_id(cx, &reading), Some(1));
    assert_eq!(comment_count(cx, &reading), Some(1));
}

#[gpui_kit::test]
fn selecting_an_unread_item_sets_last_viewed_at(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, _) = panes(cx, handle, &ws);
    assert_eq!(viewed(cx, 1), None);
    select(cx, handle, &queue, "102");
    wait_for(cx, "last_viewed_at", |cx| viewed(cx, 1).is_some());
}

#[gpui_kit::test]
fn selecting_an_unread_inbox_row_marks_the_notification_read(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, reading) = panes(cx, handle, &ws);
    // `101` is the first row; nothing before it is selected.
    select(cx, handle, &queue, "101");
    let unread = |cx: &mut TestAppContext| {
        read_db(cx, |c| c.query_row("SELECT unread FROM notifications WHERE id = '101'", [], |r| r.get::<_, i64>(0)))
    };
    wait_for(cx, "notification read", |cx| unread(cx) == 0);
    let queued = actions(cx);
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].0, "notification_read");
    assert!(queued[0].1.contains("101"), "payload: {}", queued[0].1);
    // The queue reloads and shows the row as read.
    wait_for(cx, "row shown as read", |cx| cx.read(|cx| reading.read(cx).item.as_ref().is_some_and(|r| !r.unread)));
}

#[gpui_kit::test]
fn selecting_a_read_inbox_row_enqueues_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, _) = panes(cx, handle, &ws);
    select(cx, handle, &queue, "103");
    // `101` and `102` were unread on the way: two actions, none for `103`.
    wait_for(cx, "actions", |cx| actions(cx).len() >= 2);
    cx.run_until_parked();
    let queued = actions(cx);
    assert_eq!(queued.len(), 2, "{queued:?}");
    assert!(queued.iter().all(|(_, payload)| !payload.contains("103")), "{queued:?}");
}

#[gpui_kit::test]
fn nothing_is_marked_at_launch(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_handle, _ws) = start(cx, dir.path());
    cx.run_until_parked();
    assert!(actions(cx).is_empty());
    for id in [1, 3, 5] {
        assert_eq!(viewed(cx, id), None);
    }
}

#[gpui_kit::test]
fn enter_on_a_pull_request_opens_a_reading_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    press(cx, handle, "j");
    press(cx, handle, "enter");
    let reading = active_pane(cx, handle, &ws);
    wait_for(cx, "PR detail", |cx| is_pr_detail(cx, &reading));
    assert_eq!(detail_id(cx, &reading), Some(3));
}

#[gpui_kit::test]
fn enter_on_an_issue_opens_a_reading_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    press(cx, handle, "j");
    press(cx, handle, "j");
    press(cx, handle, "enter");
    let reading = active_pane(cx, handle, &ws);
    wait_for(cx, "issue detail", |cx| comment_count(cx, &reading).is_some());
    assert_eq!(detail_id(cx, &reading), Some(1));
}

#[gpui_kit::test]
fn a_tab_for_an_unknown_pull_request_is_missing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open(ViewRef::Pr { id: 999 }, window, cx));
    })
    .unwrap();
    let reading = active_pane(cx, handle, &ws);
    wait_for(cx, "missing", |cx| cx.read(|cx| reading.read(cx).missing));
    cx.read(|cx| assert!(reading.read(cx).detail.is_none()));
}

#[gpui_kit::test]
fn a_pull_request_without_details_shows_as_its_conversation(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, reading) = panes(cx, handle, &ws);
    select(cx, handle, &queue, "106");
    wait_for(cx, "issue detail", |cx| comment_count(cx, &reading).is_some());
    assert_eq!(detail_id(cx, &reading), Some(5));
    cx.read(|cx| assert!(!reading.read(cx).missing));
}

#[gpui_kit::test]
fn a_row_without_a_cached_item_has_no_detail(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, reading) = panes(cx, handle, &ws);
    select(cx, handle, &queue, "105");
    cx.run_until_parked();
    cx.read(|cx| {
        let pane = reading.read(cx);
        assert!(pane.item.is_some());
        assert!(pane.target.is_none());
        assert!(pane.detail.is_none());
    });
}

#[gpui_kit::test]
fn the_last_selection_wins_over_a_read_in_flight(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, reading) = panes(cx, handle, &ws);
    press(cx, handle, "j");
    press(cx, handle, "j");
    wait_for(cx, "detail of the second row", |cx| detail_id(cx, &reading) == Some(1));
    cx.run_until_parked();
    assert_eq!(detail_id(cx, &reading), Some(1));
}

#[gpui_kit::test]
fn another_item_returns_to_the_conversation_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, reading) = panes(cx, handle, &ws);
    select(cx, handle, &queue, "101");
    // Let the reload after the unread mark settle: a reload keeps the tab.
    wait_for(cx, "PR read after viewing", |cx| {
        cx.read(|cx| reading.read(cx).detail.as_ref().is_some_and(|d| !d.row().unread))
    });
    reading.update(cx, |pane, cx| pane.set_tab(PaneTab::Commits, cx));
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(reading.read(cx).tab, PaneTab::Commits));
    select(cx, handle, &queue, "102");
    cx.read(|cx| assert_eq!(reading.read(cx).tab, PaneTab::Conversation));
}

#[gpui_kit::test]
fn files_of_a_pull_request_without_a_clone_fail(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, reading) = panes(cx, handle, &ws);
    select(cx, handle, &queue, "101");
    wait_for(cx, "PR detail", |cx| is_pr_detail(cx, &reading));
    reading.update(cx, |pane, cx| pane.set_tab(PaneTab::Files, cx));
    wait_for(cx, "files failed", |cx| {
        cx.read(|cx| {
            let files = reading.read(cx).files.clone();
            files.is_some_and(|f| matches!(f.read(cx).load, files::Load::Failed(_)))
        })
    });
}

#[gpui_kit::test]
fn changed_issues_reload_the_pane(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, reading) = panes(cx, handle, &ws);
    select(cx, handle, &queue, "102");
    // Wait for the reload that follows the unread mark, so it cannot land after the insert.
    wait_for(cx, "issue read after viewing", |cx| {
        cx.read(|cx| reading.read(cx).detail.as_ref().is_some_and(|d| !d.row().unread))
    });
    cx.update(|cx| Store::global(cx).read(cx).db().write(|c| {
        c.execute("INSERT INTO comments(id,issue_id,author_id,body_md,created_at,updated_at) VALUES (3,1,2,'new',2,2)", [])
    }))
    .unwrap();
    cx.update(|cx| Store::emit_changed(&["issues"], cx));
    wait_for(cx, "second comment", |cx| comment_count(cx, &reading) == Some(2));
    cx.read(|cx| assert_eq!(reading.read(cx).tab, PaneTab::Conversation));
}

// ---- action bar and keys (step 4) ------------------------------------------

fn kinds(cx: &mut TestAppContext) -> Vec<String> {
    actions(cx).into_iter().map(|(kind, _)| kind).collect()
}

fn notification<T: rusqlite::types::FromSql>(cx: &mut TestAppContext, id: &str, column: &str) -> T {
    let sql = format!("SELECT {column} FROM notifications WHERE id = ?1");
    read_db(cx, move |c| c.query_row(&sql, [id], |r| r.get(0)))
}

fn reply_open(cx: &mut TestAppContext, reading: &Entity<ReadingPane>) -> bool {
    cx.read(|cx| reading.read(cx).reply.is_some())
}

/// Select `key` and wait until its item is on show.
fn show(cx: &mut TestAppContext, handle: AnyWindowHandle, ws: &Entity<Workspace>, key: &str, id: i64) -> (Entity<Queue>, Entity<ReadingPane>) {
    let (queue, reading) = panes(cx, handle, ws);
    select(cx, handle, &queue, key);
    wait_for(cx, "detail", |cx| detail_id(cx, &reading) == Some(id));
    (queue, reading)
}

#[gpui_kit::test]
fn a_approves_the_pull_request(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    show(cx, handle, &ws, "101", 3);
    press(cx, handle, "a");
    wait_for(cx, "approve queued", |cx| kinds(cx).contains(&"submit_review".to_owned()));
    let review = actions(cx).into_iter().find(|(kind, _)| kind == "submit_review").unwrap().1;
    assert!(review.contains("APPROVE"), "payload: {review}");
    // Applied at once: a local review row.
    let state: String = read_db(cx, |c| c.query_row("SELECT state FROM reviews WHERE pr_id = 3", [], |r| r.get(0)));
    assert_eq!(state, "APPROVED");
}

#[gpui_kit::test]
fn a_on_an_issue_queues_nothing(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    show(cx, handle, &ws, "102", 1);
    press(cx, handle, "a");
    cx.run_until_parked();
    assert!(!kinds(cx).contains(&"submit_review".to_owned()), "{:?}", kinds(cx));
}

#[gpui_kit::test]
fn r_opens_a_reply_and_ctrl_enter_sends_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, reading) = show(cx, handle, &ws, "101", 3);
    press(cx, handle, "r");
    assert!(reply_open(cx, &reading));
    // The action keys type text in the box; they do not act.
    cx.update_window(handle, |_, window, cx| window.input("lgtm, ahead", cx)).unwrap();
    press(cx, handle, "ctrl-enter");
    assert!(!reply_open(cx, &reading));
    wait_for(cx, "comment queued", |cx| kinds(cx).contains(&"comment".to_owned()));
    let kinds = kinds(cx);
    assert!(!kinds.contains(&"submit_review".to_owned()), "{kinds:?}");
    let comment = actions(cx).into_iter().find(|(kind, _)| kind == "comment").unwrap().1;
    assert!(comment.contains("lgtm, ahead"), "payload: {comment}");
    // Focus is back on the queue: `j` moves the selection again.
    press(cx, handle, "j");
    cx.read(|cx| assert_eq!(queue.read(cx).selected_key(), Some("102")));
}

#[gpui_kit::test]
fn escape_closes_the_reply_without_sending(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, reading) = show(cx, handle, &ws, "102", 1);
    press(cx, handle, "r");
    cx.update_window(handle, |_, window, cx| window.input("draft", cx)).unwrap();
    press(cx, handle, "escape");
    cx.run_until_parked();
    assert!(!reply_open(cx, &reading));
    assert!(!kinds(cx).contains(&"comment".to_owned()), "{:?}", kinds(cx));
}

#[gpui_kit::test]
fn e_marks_the_row_done_and_selects_the_next(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, reading) = show(cx, handle, &ws, "102", 1);
    press(cx, handle, "e");
    wait_for(cx, "row gone", |cx| cx.read(|cx| !queue.read(cx).keys().contains(&"102")));
    assert!(notification::<Option<i64>>(cx, "102", "done_at").is_some());
    // Done is local: nothing for GitHub beyond the read marks.
    assert!(kinds(cx).iter().all(|k| k == "notification_read"), "{:?}", kinds(cx));
    // `103` took its place.
    cx.read(|cx| {
        assert_eq!(queue.read(cx).selected_key(), Some("103"));
        assert_eq!(reading.read(cx).item.as_ref().map(|r| r.key.as_str()), Some("103"));
    });
}

#[gpui_kit::test]
fn h_snoozes_the_row_for_a_day(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, _) = show(cx, handle, &ws, "102", 1);
    press(cx, handle, "h");
    wait_for(cx, "row gone", |cx| cx.read(|cx| !queue.read(cx).keys().contains(&"102")));
    let until: i64 = notification::<Option<i64>>(cx, "102", "snoozed_until").unwrap();
    let left = until - fl_sync::now();
    assert!((86_000..=86_400).contains(&left), "snoozed for {left} s");
}

#[gpui_kit::test]
fn done_and_snooze_need_an_inbox_row(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open(ViewRef::Issue { id: 1 }, window, cx));
    })
    .unwrap();
    let reading = active_pane(cx, handle, &ws);
    wait_for(cx, "issue detail", |cx| detail_id(cx, &reading) == Some(1));
    press(cx, handle, "e");
    press(cx, handle, "h");
    cx.run_until_parked();
    let marked: i64 = read_db(cx, |c| {
        c.query_row("SELECT COUNT(*) FROM notifications WHERE done_at IS NOT NULL OR snoozed_until IS NOT NULL", [], |r| r.get(0))
    });
    assert_eq!(marked, 0);
}
