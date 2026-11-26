use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use fl_types::{CheckState, IssueState, ReviewEvent};
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
pub(super) use crate::test_support::wait_for;

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
    // `101` is the first row; nothing before it is selected. It counts as
    // read once it stays selected for `READ_AFTER`.
    select(cx, handle, &queue, "101");
    cx.run_until_parked();
    assert!(actions(cx).is_empty());
    cx.executor().advance_clock(crate::views::list_view::READ_AFTER);
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
    // `101` and `102` were unread on the way, but j / k skimming marks
    // nothing; `103` is read already.
    cx.executor().advance_clock(crate::views::list_view::READ_AFTER);
    cx.run_until_parked();
    let queued = actions(cx);
    assert!(queued.is_empty(), "{queued:?}");
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
    // Done goes to GitHub too, for this thread.
    wait_for(cx, "done queued", |cx| kinds(cx).contains(&"notification_done".to_owned()));
    let done = actions(cx).into_iter().find(|(kind, _)| kind == "notification_done").unwrap().1;
    assert!(done.contains("\"102\""), "payload: {done}");
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

/// Open `view` in a tab and wait for its detail.
fn open_item(cx: &mut TestAppContext, view: ViewRef, id: i64) -> (AnyWindowHandle, Entity<ReadingPane>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open(view, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    let reading = active_pane(cx, handle, &ws);
    wait_for(cx, "detail", |cx| detail_id(cx, &reading) == Some(id));
    (handle, reading, dir)
}

fn issue_state(cx: &mut TestAppContext, id: i64) -> String {
    read_db(cx, |c| c.query_row("SELECT state FROM issues WHERE id = ?1", [id], |r| r.get(0)))
}

fn shown_state(cx: &mut TestAppContext, reading: &Entity<ReadingPane>) -> Option<IssueState> {
    cx.read(|cx| reading.read(cx).detail.as_ref().map(|d| d.row().state))
}

#[gpui_kit::test]
fn x_closes_then_reopens_the_item(cx: &mut TestAppContext) {
    let (handle, reading, _dir) = open_item(cx, ViewRef::Issue { id: 1 }, 1);
    press(cx, handle, "x");
    // Applied at once; the pane reloads on `issues` and offers Reopen.
    wait_for(cx, "closed", |cx| shown_state(cx, &reading) == Some(IssueState::Closed));
    assert_eq!(issue_state(cx, 1), "closed");
    let shown = cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.find_all("action-reopen").len()
    });
    assert_eq!(shown.unwrap(), 1);
    press(cx, handle, "x");
    wait_for(cx, "open", |cx| shown_state(cx, &reading) == Some(IssueState::Open));
    assert_eq!(issue_state(cx, 1), "open");
    let states: Vec<String> =
        actions(cx).into_iter().filter(|(kind, _)| kind == "set_state").map(|(_, payload)| payload).collect();
    // The reopen supersedes the close that was never sent.
    assert!(states.last().is_some_and(|p| p.contains("open")), "{states:?}");
}

#[gpui_kit::test]
fn merge_waits_for_the_confirm(cx: &mut TestAppContext) {
    let (handle, reading, _dir) = open_item(cx, ViewRef::Pr { id: 3 }, 3);
    cx.read(|cx| assert_eq!(reading.read(cx).merge_state(), action_bar::MergeState::Ready));
    reading.update(cx, |pane, cx| pane.pick_merge(fl_types::MergeMethod::Squash, cx));
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    assert!(!kinds(cx).contains(&"merge".to_owned()), "picking a method sends nothing");
    cx.update_window(handle, |_, window, cx| {
        window.click("action-merge-confirm", cx);
        window.render_frame(cx);
    })
    .unwrap();
    wait_for(cx, "merge queued", |cx| kinds(cx).contains(&"merge".to_owned()));
    let merge = actions(cx).into_iter().find(|(kind, _)| kind == "merge").unwrap().1;
    assert!(merge.contains("squash"), "payload: {merge}");
    assert_eq!(issue_state(cx, 3), "merged");
    // A merged PR has no Merge, Close or Approve.
    wait_for(cx, "merged", |cx| shown_state(cx, &reading) == Some(IssueState::Merged));
    cx.read(|cx| assert_eq!(reading.read(cx).merge_state(), action_bar::MergeState::None));
}

#[gpui_kit::test]
fn merge_cancel_and_other_items_drop_the_pick(cx: &mut TestAppContext) {
    let (handle, reading, _dir) = open_item(cx, ViewRef::Pr { id: 3 }, 3);
    reading.update(cx, |pane, cx| pane.pick_merge(fl_types::MergeMethod::Rebase, cx));
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click("action-merge-cancel", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.read(|cx| assert!(reading.read(cx).merge_confirm.is_none()));
    assert!(!kinds(cx).contains(&"merge".to_owned()));
}

#[gpui_kit::test]
fn merge_is_blocked_for_a_draft_or_a_conflict(cx: &mut TestAppContext) {
    let (_, reading, _dir) = open_item(cx, ViewRef::Pr { id: 3 }, 3);
    crate::test_support::write(cx, "UPDATE pr_details SET is_draft = 1 WHERE issue_id = 3", &["issues"]);
    wait_for(cx, "draft", |cx| {
        cx.read(|cx| reading.read(cx).merge_state() == action_bar::MergeState::Blocked("Draft"))
    });
    crate::test_support::write(
        cx,
        "UPDATE pr_details SET is_draft = 0, mergeable = 'CONFLICTING' WHERE issue_id = 3",
        &["issues"],
    );
    wait_for(cx, "conflict", |cx| {
        cx.read(|cx| reading.read(cx).merge_state() == action_bar::MergeState::Blocked("Has conflicts"))
    });
    // Confirm re-checks: a pick made before the block sends nothing.
    reading.update(cx, |pane, cx| {
        pane.pick_merge(fl_types::MergeMethod::Merge, cx);
        pane.confirm_merge(cx);
    });
    cx.run_until_parked();
    assert!(!kinds(cx).contains(&"merge".to_owned()));
}

// ---- review composer (parity step 2) ---------------------------------------

fn count(cx: &mut TestAppContext, handle: AnyWindowHandle, id: &'static str) -> usize {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.find_all(id).len()
    })
    .unwrap()
}

fn click(cx: &mut TestAppContext, handle: AnyWindowHandle, id: &'static str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn request_changes_sends_the_body_as_a_review(cx: &mut TestAppContext) {
    let (handle, reading, _dir) = open_item(cx, ViewRef::Pr { id: 3 }, 3);
    press(cx, handle, "r");
    cx.update_window(handle, |_, window, cx| window.input("please split this", cx)).unwrap();
    click(cx, handle, "review-request-changes");
    assert!(!reply_open(cx, &reading));
    wait_for(cx, "review queued", |cx| kinds(cx).contains(&"submit_review".to_owned()));
    let review = actions(cx).into_iter().find(|(kind, _)| kind == "submit_review").unwrap().1;
    assert!(review.contains("REQUEST_CHANGES") && review.contains("please split this"), "payload: {review}");
    assert!(!kinds(cx).contains(&"comment".to_owned()));
    let state: String = read_db(cx, |c| c.query_row("SELECT state FROM reviews WHERE pr_id = 3", [], |r| r.get(0)));
    assert_eq!(state, "CHANGES_REQUESTED");
}

#[gpui_kit::test]
fn approve_from_the_composer_keeps_the_body(cx: &mut TestAppContext) {
    let (handle, _, _dir) = open_item(cx, ViewRef::Pr { id: 3 }, 3);
    press(cx, handle, "r");
    cx.update_window(handle, |_, window, cx| window.input("nice work", cx)).unwrap();
    click(cx, handle, "review-approve");
    wait_for(cx, "review queued", |cx| kinds(cx).contains(&"submit_review".to_owned()));
    let review = actions(cx).into_iter().find(|(kind, _)| kind == "submit_review").unwrap().1;
    assert!(review.contains("APPROVE") && review.contains("nice work"), "payload: {review}");
}

#[gpui_kit::test]
fn request_changes_needs_a_body(cx: &mut TestAppContext) {
    let (handle, reading, _dir) = open_item(cx, ViewRef::Pr { id: 3 }, 3);
    press(cx, handle, "r");
    cx.update_window(handle, |_, window, cx| {
        reading.update(cx, |pane, cx| pane.send_review(ReviewEvent::RequestChanges, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert!(reply_open(cx, &reading), "an empty request keeps the box open");
    assert!(!kinds(cx).contains(&"submit_review".to_owned()), "{:?}", kinds(cx));
}

#[gpui_kit::test]
fn an_issue_reply_has_no_review_buttons(cx: &mut TestAppContext) {
    let (handle, _, _dir) = open_item(cx, ViewRef::Issue { id: 1 }, 1);
    press(cx, handle, "r");
    assert_eq!(count(cx, handle, "reply-send"), 1);
    assert_eq!(count(cx, handle, "review-approve"), 0);
    assert_eq!(count(cx, handle, "review-request-changes"), 0);
}

// ---- labels and assignees (parity step 3) ----------------------------------

const LABELS: &str = "INSERT INTO labels(id, repo_id, name, color) VALUES (1, 1, 'bug', 'd73a4a'), (2, 1, 'docs', '0075ca'),
    (3, 2, 'other repo', 'ffffff');";

fn picker_kind(cx: &mut TestAppContext, reading: &Entity<ReadingPane>) -> Option<pickers::PickerKind> {
    cx.read(|cx| reading.read(cx).picker.as_ref().map(|p| p.kind))
}

fn choices_loaded(cx: &mut TestAppContext, reading: &Entity<ReadingPane>) -> bool {
    cx.read(|cx| reading.read(cx).picker.as_ref().is_some_and(|p| p.choices.is_some()))
}

fn payloads(cx: &mut TestAppContext, kind: &str) -> Vec<String> {
    actions(cx).into_iter().filter(|(k, _)| k == kind).map(|(_, p)| p).collect()
}

fn click_pick(cx: &mut TestAppContext, handle: AnyWindowHandle, id: usize) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(("pick", id), cx);
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn l_opens_the_labels_of_the_repo_and_a_click_sends_the_set(cx: &mut TestAppContext) {
    let (handle, reading, _dir) = open_item(cx, ViewRef::Issue { id: 1 }, 1);
    crate::test_support::write(cx, LABELS, &[]);
    press(cx, handle, "l");
    assert_eq!(picker_kind(cx, &reading), Some(pickers::PickerKind::Labels));
    wait_for(cx, "choices", |cx| choices_loaded(cx, &reading));
    // The labels of this repo only.
    let ids: Vec<i64> = cx.read(|cx| reading.read(cx).visible_choices(cx).iter().map(|c| c.id).collect());
    assert_eq!(ids, [1, 2]);

    click_pick(cx, handle, 1);
    wait_for(cx, "labels queued", |cx| !payloads(cx, "set_labels").is_empty());
    // Applied at once; the header marks the field pending.
    wait_for(cx, "pending", |cx| cx.read(|cx| reading.read(cx).field_pending(pickers::PickerKind::Labels)));
    assert_eq!(count(cx, handle, "labels-pending"), 1);
    let on: i64 = crate::test_support::query_i64(cx, "SELECT COUNT(*) FROM issue_labels WHERE issue_id = 1 AND label_id = 1");
    assert_eq!(on, 1);

    // A second click sends the whole set and supersedes the first.
    click_pick(cx, handle, 2);
    wait_for(cx, "second set", |cx| payloads(cx, "set_labels").last().is_some_and(|p| p.contains("[1,2]")));
    let pending: i64 = crate::test_support::query_i64(
        cx,
        "SELECT COUNT(*) FROM pending_fields WHERE table_name = 'issues' AND row_id = 1 AND field = 'labels'",
    );
    assert_eq!(pending, 1);
}

#[gpui_kit::test]
fn the_assignee_search_and_enter_pick_the_first_match(cx: &mut TestAppContext) {
    let (handle, reading, _dir) = open_item(cx, ViewRef::Issue { id: 1 }, 1);
    click(cx, handle, "action-assignees");
    assert_eq!(picker_kind(cx, &reading), Some(pickers::PickerKind::Assignees));
    wait_for(cx, "choices", |cx| choices_loaded(cx, &reading));
    // Keys type into the search box; `l` does not act.
    cx.update_window(handle, |_, window, cx| window.input("BO", cx)).unwrap();
    let ids: Vec<i64> = cx.read(|cx| reading.read(cx).visible_choices(cx).iter().map(|c| c.id).collect());
    assert_eq!(ids, [2]);
    press(cx, handle, "enter");
    wait_for(cx, "assignees queued", |cx| !payloads(cx, "set_assignees").is_empty());
    assert!(payloads(cx, "set_assignees")[0].contains("[2]"), "{:?}", payloads(cx, "set_assignees"));
    wait_for(cx, "assigned", |cx| cx.read(|cx| reading.read(cx).assignees.iter().any(|u| u.login == "bob")));
}

#[gpui_kit::test]
fn escape_closes_the_picker_and_another_item_drops_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, reading) = show(cx, handle, &ws, "101", 3);
    press(cx, handle, "l");
    assert!(picker_kind(cx, &reading).is_some());
    press(cx, handle, "escape");
    assert!(picker_kind(cx, &reading).is_none());
    // Focus is back on the queue.
    press(cx, handle, "j");
    cx.read(|cx| assert_eq!(queue.read(cx).selected_key(), Some("102")));
    wait_for(cx, "issue", |cx| detail_id(cx, &reading) == Some(1));
    press(cx, handle, "l");
    assert!(picker_kind(cx, &reading).is_some());
    reading.update(cx, |pane, cx| pane.set_item(None, cx));
    assert!(picker_kind(cx, &reading).is_none());
    assert!(payloads(cx, "set_labels").is_empty());
}

// ---- merge method per repo, check state (parity G1 part 3) -----------------

fn open_pr_in(cx: &mut TestAppContext, handle: AnyWindowHandle, ws: &Entity<Workspace>, id: i64) -> Entity<ReadingPane> {
    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open(ViewRef::Pr { id }, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    let reading = active_pane(cx, handle, ws);
    wait_for(cx, "detail", |cx| detail_id(cx, &reading) == Some(id) && is_pr_detail(cx, &reading));
    reading
}

fn saved_method(cx: &mut TestAppContext, repo_id: i64) -> Option<String> {
    read_db(cx, move |c| {
        use rusqlite::OptionalExtension as _;
        c.query_row("SELECT method FROM repo_merge_methods WHERE repo_id = ?1", [repo_id], |r| r.get(0)).optional()
    })
}

#[gpui_kit::test]
fn the_merge_button_offers_the_method_last_sent_in_the_repo(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    crate::test_support::write(
        cx,
        "INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,author_id,created_at,updated_at) VALUES
           (7,'I',2,3,1,'open','second PR',2,0,40), (8,'I',1,2,1,'open','other repo',2,0,40);
         INSERT INTO pr_details(issue_id,base_ref,head_ref,base_oid,head_oid,changed_files)
           VALUES (7,'main','t2','b1','h7',1), (8,'main','t3','b1','h8',1);",
        &["issues"],
    );
    // First merge in the repo: a merge commit.
    let reading = open_pr_in(cx, handle, &ws, 3);
    cx.read(|cx| assert_eq!(reading.read(cx).merge_method, fl_types::MergeMethod::Merge));
    click(cx, handle, "action-merge");
    cx.read(|cx| assert_eq!(reading.read(cx).merge_confirm, Some(fl_types::MergeMethod::Merge)));
    click(cx, handle, "action-merge-cancel");
    // Another method from the menu; saved when sent, before GitHub answers.
    reading.update(cx, |pane, cx| pane.pick_merge(fl_types::MergeMethod::Squash, cx));
    click(cx, handle, "action-merge-confirm");
    wait_for(cx, "method saved", |cx| saved_method(cx, 2).as_deref() == Some("squash"));
    assert!(actions(cx).into_iter().any(|(k, p)| k == "merge" && p.contains("squash")));

    // The next PR in that repo offers squash; a PR in another repo does not.
    let next = open_pr_in(cx, handle, &ws, 7);
    cx.read(|cx| assert_eq!(next.read(cx).merge_method, fl_types::MergeMethod::Squash));
    click(cx, handle, "action-merge");
    cx.read(|cx| assert_eq!(next.read(cx).merge_confirm, Some(fl_types::MergeMethod::Squash)));
    let other = open_pr_in(cx, handle, &ws, 8);
    cx.read(|cx| assert_eq!(other.read(cx).merge_method, fl_types::MergeMethod::Merge));
}

#[gpui_kit::test]
fn the_check_state_shows_beside_merge(cx: &mut TestAppContext) {
    let (handle, reading, _dir) = open_item(cx, ViewRef::Pr { id: 3 }, 3);
    assert_eq!(count(cx, handle, "merge-checks"), 0, "no combined state yet");
    crate::test_support::write(cx, "UPDATE pr_details SET check_state = 'FAILURE' WHERE issue_id = 3", &["issues"]);
    wait_for(cx, "checks", |cx| cx.read(|cx| reading.read(cx).check_state() == Some(CheckState::Failure)));
    assert_eq!(count(cx, handle, "merge-checks"), 1);
    // Also beside a blocked Merge.
    crate::test_support::write(cx, "UPDATE pr_details SET is_draft = 1 WHERE issue_id = 3", &["issues"]);
    wait_for(cx, "draft", |cx| {
        cx.read(|cx| reading.read(cx).merge_state() == action_bar::MergeState::Blocked("Draft"))
    });
    assert_eq!(count(cx, handle, "merge-checks"), 1);
}

/// A wheel notch over the Conversation scrolls over a few frames, not in one
/// jump, and ends where the jump would have.
#[gpui_kit::test]
fn a_wheel_notch_scrolls_the_conversation_smoothly(cx: &mut TestAppContext) {
    use gpui_kit::{ScrollDelta, point};
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    let comments: String = (10..40)
        .map(|id| {
            format!(
                "INSERT INTO comments(id,issue_id,author_id,body_md,created_at,updated_at) \
                 VALUES ({id},3,2,'one\n\ntwo\n\nthree',{id},{id});"
            )
        })
        .collect();
    let (handle, ws) = open_seeded(cx, dir.path(), &format!("{SEED}{comments}"));
    cx.run_until_parked();
    let (_, reading) = show(cx, handle, &ws, "101", 3);
    let frame = |cx: &mut TestAppContext| cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    let top = |cx: &mut TestAppContext| cx.read(|cx| reading.read(cx).conversation.scroll_top());
    frame(cx);
    assert_eq!(top(cx), 0.);

    // One notch: three lines of 20 px.
    cx.update_window(handle, |_, window, cx| window.scroll("conversation", ScrollDelta::Lines(point(0., -3.)), cx))
        .unwrap();
    let first = top(cx);
    assert!(first < 59., "the notch landed in one frame: {first}");
    let start = Instant::now();
    while top(cx) < 59.5 && start.elapsed() < Duration::from_secs(2) {
        frame(cx);
    }
    assert!((top(cx) - 60.).abs() < 0.5, "ended at {}", top(cx));
}
