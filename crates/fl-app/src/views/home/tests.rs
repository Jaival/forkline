use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, AnyWindowHandle, Entity, TestAppContext};

use super::*;
use crate::test_support::{active, open_seeded, press, start_on, tabs, write};
use crate::views::list_view::ListView;
use crate::views::queue::Queue;
use crate::workspace::Workspace;

/// The viewer is `me`. Repo 1 is mine and pinned; repo 2 is someone else's
/// and reached only through a search hit.
const SEED: &str = r#"
    INSERT INTO settings(key, value_json) VALUES ('viewer', '{"login":"me"}');
    INSERT INTO users(id, login, avatar_url) VALUES
      (1, 'me', 'https://avatars.githubusercontent.com/u/1'),
      (2, 'dependabot', 'https://avatars.githubusercontent.com/in/29110'),
      (3, 'bob', 'https://avatars.githubusercontent.com/u/3');
    INSERT INTO repos(id, node_id, owner, name, updated_at, pinned) VALUES (1,'R','me','a',0,1), (2,'R','x','b',0,0);
    INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,author_id,created_at,updated_at) VALUES
      (1,'I',1,1,1,'open','my failing pr',1,0,10),
      (2,'I',1,2,1,'open','bot bump',2,0,20),
      (3,'I',1,3,0,'open','assigned issue',3,0,30),
      (4,'I',1,4,1,'open','my green pr',1,0,40),
      (5,'I',1,5,1,'open','my pending pr',1,0,50),
      (6,'I',1,6,0,'open','other issue',3,0,60),
      (7,'I',2,7,1,'open','review me',3,0,70),
      (8,'I',1,8,0,'closed','closed issue',3,0,80),
      (9,'I',2,9,0,'open','unrelated',3,0,90);
    INSERT INTO pr_details(issue_id, base_ref, head_ref, base_oid, head_oid, check_state) VALUES
      (1,'main','a','x','y','FAILURE'), (2,'main','b','x','y','SUCCESS'),
      (4,'main','c','x','y','SUCCESS'), (5,'main','d','x','y','PENDING'), (7,'main','e','x','y',NULL);
    INSERT INTO issue_assignees(issue_id, user_id) VALUES (3, 1);
    INSERT INTO search_hits(query, issue_id) VALUES ('review_requested', 7);
"#;

fn queue(cx: &mut TestAppContext, handle: AnyWindowHandle, ws: &Entity<Workspace>) -> Entity<Queue> {
    let list = active::<ListView>(cx, handle, ws);
    cx.read(|cx| list.read(cx).queue.clone())
}

fn keys(cx: &mut TestAppContext, queue: &Entity<Queue>) -> Vec<String> {
    cx.read(|cx| queue.read(cx).keys().into_iter().map(str::to_owned).collect())
}

fn shown(cx: &mut TestAppContext, queue: &Entity<Queue>) -> Vec<String> {
    cx.read(|cx| queue.read(cx).shown_keys().into_iter().map(str::to_owned).collect())
}

#[gpui_kit::test]
fn groups_rank_what_needs_me_first(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), SEED, ViewRef::Home);
    let queue = queue(cx, handle, &ws);
    // Needs action, waiting on me (review request, then the bot PR),
    // assigned, ready to merge, in progress, recent. No closed item, no item
    // of an unpinned repo without a hit.
    assert_eq!(keys(cx, &queue), ["1", "7", "2", "3", "4", "5", "6"]);
    assert_eq!(cx.read(|cx| queue.read(cx).headers()), 6);
    // "My PRs in progress" and "Recently updated" start folded.
    assert_eq!(shown(cx, &queue), ["1", "7", "2", "3", "4"]);
    let bot = cx.read(|cx| queue.read(cx).rows_for_test().iter().find(|r| r.key == "2").map(|r| r.bot));
    assert_eq!(bot, Some(true));
}

#[gpui_kit::test]
fn j_skips_folded_groups_and_enter_opens(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), SEED, ViewRef::Home);
    let queue = queue(cx, handle, &ws);
    for _ in 0..8 {
        press(cx, handle, "j");
    }
    assert_eq!(cx.read(|cx| queue.read(cx).selected_key().map(str::to_owned)).as_deref(), Some("4"));
    press(cx, handle, "enter");
    assert_eq!(tabs(cx, &ws).last().map(String::as_str), Some("*#4 my green pr"));
}

#[gpui_kit::test]
fn clicking_a_folded_header_shows_its_rows(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), SEED, ViewRef::Home);
    let queue = queue(cx, handle, &ws);
    let ix = fl_types::QueueGroup::ALL.iter().position(|g| *g == fl_types::QueueGroup::InProgress).unwrap();
    cx.update_window(handle, |_, window, cx| {
        window.click(("queue-group", ix), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(shown(cx, &queue), ["1", "7", "2", "3", "4", "5"]);
}

#[gpui_kit::test]
fn a_new_failure_moves_the_pr_up(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), SEED, ViewRef::Home);
    let queue = queue(cx, handle, &ws);
    write(cx, "UPDATE pr_details SET check_state = 'FAILURE' WHERE issue_id = 4", &["pr"]);
    assert_eq!(keys(cx, &queue)[..2], ["4", "1"]);
}

#[gpui_kit::test]
fn launch_opens_home_unless_turned_off(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    // The test helper turns it off; this seed turns it back on.
    let (_handle, ws) = open_seeded(cx, dir.path(), "DELETE FROM settings WHERE key = 'open_home';");
    cx.run_until_parked();
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Home"]);
}

#[gpui_kit::test]
fn launch_keeps_the_last_tab_when_turned_off(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_handle, ws) = open_seeded(cx, dir.path(), "");
    cx.run_until_parked();
    assert_eq!(tabs(cx, &ws), ["*Inbox"]);
}

#[gpui_kit::test]
fn the_auto_pin_note_shows_until_closed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let seed = format!("{SEED} INSERT INTO settings(key, value_json) VALUES ('auto_pin_note', '3');");
    let (handle, ws) = start_on(cx, dir.path(), &seed, ViewRef::Home);
    let list = active::<ListView>(cx, handle, &ws);
    assert_eq!(cx.read(|cx| list.read(cx).pin_note), Some(3));
    cx.update_window(handle, |_, window, cx| {
        window.click("home-pin-close", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cx.read(|cx| list.read(cx).pin_note), None);
    crate::test_support::wait_for(cx, "note deleted", |cx| {
        crate::test_support::query_i64(cx, "SELECT count(*) FROM settings WHERE key = 'auto_pin_note'") == 0
    });
}

#[test]
fn the_note_names_the_count() {
    assert!(auto_pin_text(1).starts_with("Pinned 1 repo you pushed"));
    assert!(auto_pin_text(10).starts_with("Pinned 10 repos you pushed"));
}

#[gpui_kit::test]
fn the_note_appears_when_auto_pin_runs_after_launch(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), SEED, ViewRef::Home);
    let list = active::<ListView>(cx, handle, &ws);
    assert_eq!(cx.read(|cx| list.read(cx).pin_note), None);
    write(cx, "INSERT INTO settings(key, value_json) VALUES ('auto_pin_note', '10')", &["repos"]);
    assert_eq!(cx.read(|cx| list.read(cx).pin_note), Some(10));
}
