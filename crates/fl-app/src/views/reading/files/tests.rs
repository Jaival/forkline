use std::path::Path;
use std::process::Command;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::{FilesTab, Load};
use crate::test_support::open_seeded;
use crate::view_ref::ViewRef;
use crate::views::reading::tests::{SEED, active_pane, is_pr_detail, panes, press, read_db, select, wait_for};
use crate::views::reading::{PaneTab, ReadingPane};
use crate::workspace::Workspace;

const PATHS: [&str; 3] = ["a.txt", "b.txt", "src/c.rs"];

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

/// A base and a head commit that change three files, cloned bare to the path
/// the app reads offline (`<data>/git/o/b.git`). Returns (base, head).
fn make_clone(data: &Path) -> (String, String) {
    let work = data.join("work");
    std::fs::create_dir_all(work.join("src")).unwrap();
    git(&work, &["init", "-q", "-b", "main"]);
    for p in PATHS {
        std::fs::write(work.join(p), "one\ntwo\nthree\n").unwrap();
    }
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "base"]);
    let base = git(&work, &["rev-parse", "HEAD"]);
    for p in PATHS {
        std::fs::write(work.join(p), "one\nTWO\nthree\nfour\n").unwrap();
    }
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "head"]);
    let head = git(&work, &["rev-parse", "HEAD"]);
    let bare = data.join("git").join("o").join("b.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    git(data, &["clone", "-q", "--bare", work.to_str().unwrap(), bare.to_str().unwrap()]);
    (base, head)
}

fn start(cx: &mut TestAppContext, dir: &Path) -> (AnyWindowHandle, Entity<Workspace>) {
    let (base, head) = make_clone(dir);
    let sql = format!(
        "{SEED}
         UPDATE pr_details SET base_oid = '{base}', head_oid = '{head}', changed_files = 3 WHERE issue_id = 3;
         INSERT INTO review_threads(id,node_id,pr_id,path,side,line,is_resolved) VALUES (1,'T',3,'a.txt','RIGHT',2,0);
         INSERT INTO review_comments(id,thread_id,author_id,body_md,created_at) VALUES (1,1,2,'please check',1);"
    );
    let (handle, ws) = open_seeded(cx, dir, &sql);
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    (handle, ws)
}

fn ready(cx: &mut TestAppContext, tab: &Entity<FilesTab>) -> bool {
    cx.read(|cx| matches!(tab.read(cx).load, Load::Ready))
}

fn files_tab(cx: &mut TestAppContext, reading: &Entity<ReadingPane>) -> Entity<FilesTab> {
    cx.read(|cx| reading.read(cx).files.clone().expect("files tab"))
}

/// Open PR 3 beside the Inbox and show its Files tab once loaded.
fn open_files(cx: &mut TestAppContext, handle: AnyWindowHandle, ws: &Entity<Workspace>) -> (Entity<ReadingPane>, Entity<FilesTab>) {
    let (queue, reading) = panes(cx, handle, ws);
    select(cx, handle, &queue, "101");
    wait_for(cx, "PR detail", |cx| is_pr_detail(cx, &reading));
    reading.update(cx, |pane, cx| pane.set_tab(PaneTab::Files, cx));
    let tab = files_tab(cx, &reading);
    wait_for(cx, "files ready", |cx| ready(cx, &tab));
    (reading, tab)
}

fn current(cx: &mut TestAppContext, tab: &Entity<FilesTab>) -> Option<usize> {
    cx.read(|cx| tab.read(cx).current)
}

fn is_viewed(cx: &mut TestAppContext, tab: &Entity<FilesTab>, path: &str) -> bool {
    cx.read(|cx| tab.read(cx).viewed.contains(path))
}

fn is_collapsed(cx: &mut TestAppContext, tab: &Entity<FilesTab>, path: &str) -> bool {
    cx.read(|cx| tab.read(cx).diff.as_ref().unwrap().read(cx).is_file_collapsed(path))
}

fn viewed_rows(cx: &mut TestAppContext) -> Vec<String> {
    read_db(cx, |c| {
        c.prepare("SELECT path FROM pr_viewed_files WHERE pr_id = 3 ORDER BY path")?
            .query_map([], |r| r.get(0))?
            .collect()
    })
}

#[gpui_kit::test]
fn the_files_tab_loads_files_diff_and_threads(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, tab) = open_files(cx, handle, &ws);
    cx.read(|cx| {
        let tab = tab.read(cx);
        let paths: Vec<&str> = tab.files.iter().map(|f| f.path().as_ref()).collect();
        assert_eq!(paths, PATHS);
        assert_eq!(tab.current, Some(0));
        assert!(tab.diff.is_some());
        assert_eq!(tab.threads.len(), 1);
        assert_eq!(tab.threads[0].path, "a.txt");
        assert_eq!(tab.file_count(), Some(3));
    });
}

#[gpui_kit::test]
fn brackets_step_between_files_and_wrap(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, tab) = open_files(cx, handle, &ws);
    press(cx, handle, "]");
    assert_eq!(current(cx, &tab), Some(1));
    press(cx, handle, "[");
    assert_eq!(current(cx, &tab), Some(0));
    press(cx, handle, "[");
    assert_eq!(current(cx, &tab), Some(2));
    press(cx, handle, "]");
    assert_eq!(current(cx, &tab), Some(0));
}

#[gpui_kit::test]
fn v_marks_a_file_viewed_and_again_unmarks_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, tab) = open_files(cx, handle, &ws);
    press(cx, handle, "v");
    assert!(is_viewed(cx, &tab, "a.txt"));
    assert!(is_collapsed(cx, &tab, "a.txt"));
    assert!(!is_collapsed(cx, &tab, "b.txt"));
    assert_eq!(current(cx, &tab), Some(1));
    wait_for(cx, "viewed row", |cx| viewed_rows(cx) == ["a.txt"]);
    let head: String = read_db(cx, |c| c.query_row("SELECT head_oid FROM pr_viewed_files WHERE pr_id = 3", [], |r| r.get(0)));
    assert_eq!(head.len(), 40);

    // Back to a.txt, then `v` again.
    press(cx, handle, "[");
    assert_eq!(current(cx, &tab), Some(0));
    press(cx, handle, "v");
    assert!(!is_viewed(cx, &tab, "a.txt"));
    assert!(!is_collapsed(cx, &tab, "a.txt"));
    wait_for(cx, "viewed row gone", |cx| viewed_rows(cx).is_empty());
    // Unmarking stays on the file.
    assert_eq!(current(cx, &tab), Some(0));
}

#[gpui_kit::test]
fn viewing_every_file_stays_on_the_last_one(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, tab) = open_files(cx, handle, &ws);
    for _ in 0..3 {
        press(cx, handle, "v");
    }
    for p in PATHS {
        assert!(is_viewed(cx, &tab, p), "{p}");
        assert!(is_collapsed(cx, &tab, p), "{p}");
    }
    assert_eq!(current(cx, &tab), Some(2));
    wait_for(cx, "three rows", |cx| viewed_rows(cx).len() == 3);
}

#[gpui_kit::test]
fn viewed_files_persist_in_a_fresh_files_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, tab) = open_files(cx, handle, &ws);
    press(cx, handle, "v");
    wait_for(cx, "viewed row", |cx| viewed_rows(cx) == ["a.txt"]);
    drop(tab);

    cx.update_window(handle, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.open(ViewRef::Pr { id: 3 }, window, cx));
    })
    .unwrap();
    let reading = active_pane(cx, handle, &ws);
    wait_for(cx, "PR detail", |cx| is_pr_detail(cx, &reading));
    reading.update(cx, |pane, cx| pane.set_tab(PaneTab::Files, cx));
    let fresh = files_tab(cx, &reading);
    wait_for(cx, "files ready", |cx| ready(cx, &fresh));
    assert!(is_viewed(cx, &fresh, "a.txt"));
    assert!(is_collapsed(cx, &fresh, "a.txt"));
    assert!(!is_viewed(cx, &fresh, "b.txt"));
    assert!(!is_collapsed(cx, &fresh, "b.txt"));
    assert_eq!(current(cx, &fresh), Some(1));
}

#[gpui_kit::test]
fn file_keys_do_nothing_outside_the_files_tab(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (queue, reading) = panes(cx, handle, &ws);
    select(cx, handle, &queue, "101");
    wait_for(cx, "PR detail", |cx| is_pr_detail(cx, &reading));
    press(cx, handle, "]");
    press(cx, handle, "[");
    press(cx, handle, "v");
    cx.run_until_parked();
    cx.read(|cx| assert!(reading.read(cx).files.is_none()));
    assert!(viewed_rows(cx).is_empty());

    // After a visit to Files, the keys stay quiet on another tab.
    reading.update(cx, |pane, cx| pane.set_tab(PaneTab::Files, cx));
    let tab = files_tab(cx, &reading);
    wait_for(cx, "files ready", |cx| ready(cx, &tab));
    reading.update(cx, |pane, cx| pane.set_tab(PaneTab::Conversation, cx));
    press(cx, handle, "]");
    press(cx, handle, "v");
    cx.run_until_parked();
    assert_eq!(current(cx, &tab), Some(0));
    assert!(!is_viewed(cx, &tab, "a.txt"));
    assert!(viewed_rows(cx).is_empty());
}

// ---- threads (parity step 4) -----------------------------------------------

fn thread(cx: &mut TestAppContext, tab: &Entity<FilesTab>) -> (bool, usize) {
    cx.read(|cx| {
        let t = &tab.read(cx).threads[0];
        (t.is_resolved, t.comments.len())
    })
}

fn kinds(cx: &mut TestAppContext) -> Vec<(String, String)> {
    read_db(cx, |c| {
        c.prepare("SELECT kind, payload_json FROM action_queue ORDER BY id")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect()
    })
}

fn click(cx: &mut TestAppContext, handle: AnyWindowHandle, id: impl Into<gpui_kit::ElementId>) {
    let id = id.into();
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
        window.render_frame(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn resolve_then_unresolve_a_thread(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, tab) = open_files(cx, handle, &ws);
    click(cx, handle, ("thread-resolve", 0usize));
    // Applied at once; the rail reloads on `pr`.
    wait_for(cx, "resolved", |cx| thread(cx, &tab).0);
    let resolve: Vec<String> =
        kinds(cx).into_iter().filter(|(k, _)| k == "resolve_thread").map(|(_, p)| p).collect();
    assert!(resolve.last().is_some_and(|p| p.contains("true")), "{resolve:?}");
    click(cx, handle, ("thread-resolve", 0usize));
    wait_for(cx, "open again", |cx| !thread(cx, &tab).0);
    let pending: i64 = read_db(cx, |c| {
        c.query_row("SELECT COUNT(*) FROM pending_fields WHERE table_name = 'review_threads'", [], |r| r.get(0))
    });
    assert_eq!(pending, 1, "the unresolve supersedes the resolve");
}

#[gpui_kit::test]
fn reply_in_a_thread_with_ctrl_enter(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, tab) = open_files(cx, handle, &ws);
    click(cx, handle, ("thread-reply", 0usize));
    cx.read(|cx| assert!(tab.read(cx).reply.is_some()));
    // Keys type into the box: `v` and `]` do not act.
    cx.update_window(handle, |_, window, cx| window.input("fixed in v2]", cx)).unwrap();
    press(cx, handle, "ctrl-enter");
    cx.read(|cx| assert!(tab.read(cx).reply.is_none()));
    wait_for(cx, "reply shown", |cx| thread(cx, &tab).1 == 2);
    let reply = kinds(cx).into_iter().find(|(k, _)| k == "reply_thread").unwrap().1;
    assert!(reply.contains("fixed in v2]"), "payload: {reply}");
    assert_eq!(current(cx, &tab), Some(0));
    assert!(!is_viewed(cx, &tab, "a.txt"));
}

#[gpui_kit::test]
fn escape_cancels_a_thread_reply(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, tab) = open_files(cx, handle, &ws);
    click(cx, handle, ("thread-reply", 0usize));
    cx.update_window(handle, |_, window, cx| window.input("draft", cx)).unwrap();
    press(cx, handle, "escape");
    cx.read(|cx| assert!(tab.read(cx).reply.is_none()));
    cx.run_until_parked();
    assert!(kinds(cx).iter().all(|(k, _)| k != "reply_thread"));
}

// ---- inline review comments (parity step 5) --------------------------------

use gpui_kit::component::diff::{DiffLineRange, DiffSide};

fn open_composer(cx: &mut TestAppContext, handle: AnyWindowHandle, tab: &Entity<FilesTab>, range: DiffLineRange) {
    cx.update_window(handle, |_, window, cx| {
        tab.update(cx, |tab, cx| tab.open_composer(range, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
}

fn drafts(cx: &mut TestAppContext, tab: &Entity<FilesTab>) -> Vec<fl_types::DraftReviewComment> {
    cx.read(|cx| tab.read(cx).drafts.clone())
}

#[test]
fn a_range_on_one_side_is_a_multi_line_draft() {
    let d = super::draft_for(&DiffLineRange::new("a.txt", DiffSide::Modified, 4, 2), "x".into());
    assert_eq!((d.side, d.line, d.start_line), (fl_types::DiffSide::Right, 4, Some(2)));
    let one = super::draft_for(&DiffLineRange::new("a.txt", DiffSide::Original, 3, 3), "x".into());
    assert_eq!((one.side, one.line, one.start_line), (fl_types::DiffSide::Left, 3, None));
    // Across sides: a comment on the last line.
    let cross = super::draft_for(
        &DiffLineRange::new("a.txt", DiffSide::Original, 2, 3).with_end_side(DiffSide::Modified),
        "x".into(),
    );
    assert_eq!((cross.side, cross.line, cross.start_line), (fl_types::DiffSide::Right, 3, None));
}

#[gpui_kit::test]
fn drafts_go_with_the_next_review(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (reading, tab) = open_files(cx, handle, &ws);
    open_composer(cx, handle, &tab, DiffLineRange::new("b.txt", DiffSide::Modified, 2, 2));
    // Keys type into the box: `v` and `]` do not act.
    cx.update_window(handle, |_, window, cx| window.input("nit: v]", cx)).unwrap();
    press(cx, handle, "ctrl-enter");
    let d = drafts(cx, &tab);
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].path.as_str(), d[0].line, d[0].body_md.as_str()), ("b.txt", 2, "nit: v]"));
    cx.read(|cx| assert!(tab.read(cx).composer.is_none()));
    assert!(!is_viewed(cx, &tab, "a.txt"));
    assert!(kinds(cx).iter().all(|(k, _)| k != "submit_review"), "a draft sends nothing");

    // `R`, then Ctrl+Enter with no text: a COMMENT review with the draft.
    press(cx, handle, "r");
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find_all("review-drafts").len(), 1);
    })
    .unwrap();
    press(cx, handle, "ctrl-enter");
    wait_for(cx, "review queued", |cx| kinds(cx).iter().any(|(k, _)| k == "submit_review"));
    let review = kinds(cx).into_iter().find(|(k, _)| k == "submit_review").unwrap().1;
    assert!(review.contains("COMMENT") && review.contains("b.txt") && review.contains("nit: v]"), "payload: {review}");
    assert!(kinds(cx).iter().all(|(k, _)| k != "comment"));
    assert!(drafts(cx, &tab).is_empty());
    // Applied at once: the draft is a thread in the rail now.
    wait_for(cx, "new thread", |cx| cx.read(|cx| tab.read(cx).threads.iter().any(|t| t.path == "b.txt")));
    cx.read(|cx| assert!(reading.read(cx).reply.is_none()));
}

#[gpui_kit::test]
fn approve_takes_the_drafts_too(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, tab) = open_files(cx, handle, &ws);
    open_composer(cx, handle, &tab, DiffLineRange::new("a.txt", DiffSide::Modified, 2, 4));
    cx.update_window(handle, |_, window, cx| window.input("these lines", cx)).unwrap();
    press(cx, handle, "ctrl-enter");
    press(cx, handle, "a");
    wait_for(cx, "review queued", |cx| kinds(cx).iter().any(|(k, _)| k == "submit_review"));
    let review = kinds(cx).into_iter().find(|(k, _)| k == "submit_review").unwrap().1;
    assert!(review.contains("APPROVE") && review.contains("\"start_line\":2"), "payload: {review}");
    assert!(drafts(cx, &tab).is_empty());
}

#[gpui_kit::test]
fn escape_cancels_the_box_and_remove_drops_a_draft(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, tab) = open_files(cx, handle, &ws);
    open_composer(cx, handle, &tab, DiffLineRange::new("a.txt", DiffSide::Modified, 2, 2));
    cx.update_window(handle, |_, window, cx| window.input("gone", cx)).unwrap();
    press(cx, handle, "escape");
    cx.read(|cx| assert!(tab.read(cx).composer.is_none()));
    assert!(drafts(cx, &tab).is_empty());

    open_composer(cx, handle, &tab, DiffLineRange::new("a.txt", DiffSide::Modified, 2, 2));
    cx.update_window(handle, |_, window, cx| window.input("kept", cx)).unwrap();
    press(cx, handle, "ctrl-enter");
    assert_eq!(drafts(cx, &tab).len(), 1);
    tab.update(cx, |tab, cx| tab.remove_draft(0, cx));
    assert!(drafts(cx, &tab).is_empty());
}

#[gpui_kit::test]
fn toolbar_comment_opens_the_box_for_the_selected_lines(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start(cx, dir.path());
    let (_, tab) = open_files(cx, handle, &ws);
    // Nothing selected: the button does nothing.
    click(cx, handle, "files-comment");
    cx.read(|cx| assert!(tab.read(cx).composer.is_none()));
    let diff = cx.read(|cx| tab.read(cx).diff.clone().unwrap());
    diff.update(cx, |diff, cx| {
        diff.set_selected_lines(Some(DiffLineRange::new("a.txt", DiffSide::Modified, 2, 3)), cx);
    });
    click(cx, handle, "files-comment");
    cx.update_window(handle, |_, window, cx| window.input("two lines", cx)).unwrap();
    press(cx, handle, "ctrl-enter");
    let d = drafts(cx, &tab);
    assert_eq!((d[0].path.as_str(), d[0].line, d[0].start_line), ("a.txt", 3, Some(2)));
}
