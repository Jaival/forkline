use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext};

use super::*;
use crate::test_support::{active, press, query_i64, start_on, tabs, write};

const SEED: &str = "
    INSERT INTO users (id, login) VALUES (1, 'ann'), (2, 'bob');
    INSERT INTO repos (id, node_id, owner, name, description, default_branch, license, stars, forks, updated_at, languages_json)
      VALUES (1, 'R1', 'acme', 'rocket', 'Goes up', 'main', 'MIT', 42, 3, 0,
              '[{\"name\":\"Rust\",\"bytes\":30},{\"name\":\"Go\",\"bytes\":10}]');
    INSERT INTO issues (id, node_id, repo_id, number, is_pr, state, title, created_at, updated_at) VALUES
      (1, 'I1', 1, 1, 1, 'open', 'pr', 0, 1), (2, 'I2', 1, 2, 0, 'open', 'bug', 0, 1), (3, 'I3', 1, 3, 0, 'open', 'bug2', 0, 1);
    INSERT INTO commits (repo_id, oid, headline, author_login, authored_at, check_state, on_default_branch)
      VALUES (1, 'abcdef1234', 'first', 'ann', 86400, 'SUCCESS', 1), (1, 'bbbbbbb999', 'second', NULL, 172800, NULL, 1);
    INSERT INTO releases (id, repo_id, tag, name, published_at) VALUES (1, 1, 'v1.0', 'One', 86400);
    INSERT INTO repo_contributors (repo_id, user_id, contributions) VALUES (1, 1, 9), (1, 2, 4);
    INSERT INTO star_history (repo_id, day, stars) VALUES (1, 1, 10), (1, 2, 20);";

fn open(cx: &mut TestAppContext, dir: &std::path::Path, repo_id: i64) -> (gpui_kit::AnyWindowHandle, Entity<crate::workspace::Workspace>, Entity<RepoView>) {
    let (handle, ws) = start_on(cx, dir, SEED, ViewRef::Repo { repo_id });
    let view = active::<RepoView>(cx, handle, &ws);
    (handle, ws, view)
}

#[test]
fn shares_sort_largest_first() {
    let langs = |l: &[(&str, u64)]| l.iter().map(|(n, b)| LanguageShare { name: (*n).into(), bytes: *b }).collect::<Vec<_>>();
    assert_eq!(language_shares(&langs(&[("Go", 1), ("Rust", 3)])), [("Rust".to_owned(), 75.), ("Go".to_owned(), 25.)]);
    assert!(language_shares(&langs(&[("Go", 0)])).is_empty());
    assert_eq!(language_color("Rust"), language_color("Rust"));
}

#[test]
fn dates_are_utc_days() {
    assert_eq!(date(0), "1970-01-01");
    assert_eq!(date(951_782_400), "2000-02-29");
    assert_eq!(date(1_763_337_600), "2025-11-17");
}

#[gpui_kit::test]
fn the_overview_loads(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, _, view) = open(cx, dir.path(), 1);
    cx.read(|cx| {
        let o = view.read(cx).overview().unwrap();
        assert_eq!((o.owner.as_str(), o.name.as_str()), ("acme", "rocket"));
        assert_eq!((o.open_prs, o.open_issues), (1, 2));
        assert_eq!(o.commits.iter().map(|c| c.headline.as_str()).collect::<Vec<_>>(), ["second", "first"]);
        assert_eq!(o.releases.len(), 1);
        assert_eq!(o.contributors[0].login, "ann");
        assert_eq!(o.star_history.len(), 2);
    });
}

#[gpui_kit::test]
fn a_missing_repo_says_so(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws, view) = open(cx, dir.path(), 99);
    cx.read(|cx| assert!(view.read(cx).is_missing()));
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Repository 99"]);
    // `p` does nothing without a repo.
    press(cx, handle, "p");
}

#[gpui_kit::test]
fn p_pins_and_unpins(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _, view) = open(cx, dir.path(), 1);
    press(cx, handle, "p");
    assert_eq!(query_i64(cx, "SELECT pinned FROM repos WHERE id = 1"), 1);
    cx.read(|cx| assert!(view.read(cx).overview().unwrap().pinned));
    press(cx, handle, "p");
    assert_eq!(query_i64(cx, "SELECT pinned FROM repos WHERE id = 1"), 0);
    cx.read(|cx| assert!(!view.read(cx).overview().unwrap().pinned));
}

#[gpui_kit::test]
fn the_pin_button_pins(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, _, _) = open(cx, dir.path(), 1);
    cx.update_window(handle, |_, window, cx| window.click("repo-pin", cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(query_i64(cx, "SELECT pinned FROM repos WHERE id = 1"), 1);
}

#[gpui_kit::test]
fn the_cards_open_the_repos_queues(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws, _) = open(cx, dir.path(), 1);
    cx.update_window(handle, |_, window, cx| window.click("repo-open-prs", cx)).unwrap();
    cx.run_until_parked();
    assert_eq!(tabs(cx, &ws), ["Inbox", "acme/rocket", "*acme/rocket: Pull requests"]);
    let list = active::<super::super::list_view::ListView>(cx, handle, &ws);
    cx.read(|cx| assert_eq!(list.read(cx).queue.read(cx).repo_id(), Some(1)));
}

#[gpui_kit::test]
fn the_tab_title_follows_a_rename(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, ws, _) = open(cx, dir.path(), 1);
    assert_eq!(tabs(cx, &ws), ["Inbox", "*acme/rocket"]);
    write(cx, "UPDATE repos SET name = 'jet' WHERE id = 1", &["repos"]);
    assert_eq!(tabs(cx, &ws), ["Inbox", "*acme/jet"]);
}

#[gpui_kit::test]
fn a_repos_change_reloads(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, _, view) = open(cx, dir.path(), 1);
    write(cx, "UPDATE repos SET description = 'Goes further' WHERE id = 1", &["repos"]);
    cx.read(|cx| assert_eq!(view.read(cx).overview().unwrap().description.as_deref(), Some("Goes further")));
}

#[gpui_kit::test]
fn picking_a_colour_shows_at_once_and_is_saved(cx: &mut TestAppContext) {
    use crate::theme::{ActivePalette as _, repo_color};
    let dir = tempfile::tempdir().unwrap();
    let (_, _, _view) = open(cx, dir.path(), 1);
    let auto = cx.read(|cx| repo_color("acme/rocket", cx));
    let picked = cx.read(|cx| (0..10).map(|i| cx.palette().repo(i)).find(|c| *c != auto).unwrap());
    let index = cx.read(|cx| (0..10u8).find(|i| cx.palette().repo(*i) == picked).unwrap());
    cx.update(|cx| crate::repo_color::set(1, "acme/rocket", Some(index), cx));
    cx.read(|cx| assert_eq!(repo_color("acme/rocket", cx), picked));
    cx.run_until_parked();
    assert_eq!(query_i64(cx, "SELECT color FROM repos WHERE id = 1"), index as i64);
    // Back to the colour from the name.
    cx.update(|cx| crate::repo_color::set(1, "acme/rocket", None, cx));
    cx.run_until_parked();
    cx.read(|cx| assert_eq!(repo_color("acme/rocket", cx), auto));
    assert_eq!(query_i64(cx, "SELECT count(*) FROM repos WHERE color IS NOT NULL"), 0);
}
