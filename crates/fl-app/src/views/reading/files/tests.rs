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
