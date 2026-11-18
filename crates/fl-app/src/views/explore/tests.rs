use gpui_kit::TestAppContext;

use super::*;
use crate::test_support::{active, press, start_on, tabs, write};

const SEED: &str = "
    INSERT INTO users (id, login) VALUES (1, 'ann'), (2, 'bob');
    INSERT INTO following (user_id) VALUES (1), (2);
    INSERT INTO repos (id, node_id, owner, name, updated_at, explore_only) VALUES
      (7, 'R7', 'o', 'old', 0, 1), (8, 'R8', 'o', 'new', 0, 1);
    INSERT INTO explore_stars (user_id, repo_id, starred_at) VALUES (1, 7, 10), (2, 7, 20), (1, 8, 30);";

fn row(by: &[&str], count: u32) -> ExploreRow {
    ExploreRow {
        repo_id: 1,
        owner: String::new(),
        name: String::new(),
        description: None,
        language: None,
        stars: 0,
        starred: false,
        starred_by: by.iter().map(|s| (*s).to_owned()).collect(),
        starred_by_count: count,
        last_starred_at: 0,
    }
}

#[test]
fn starred_by_names_up_to_three_and_counts_the_rest() {
    assert_eq!(starred_by(&row(&["ann"], 1)), "starred by ann");
    assert_eq!(starred_by(&row(&["ann", "bob", "cy"], 4)), "starred by ann, bob, cy and 1 other");
    assert_eq!(starred_by(&row(&["ann", "bob", "cy"], 6)), "starred by ann, bob, cy and 3 others");
}

#[gpui_kit::test]
fn rows_come_newest_star_first_and_enter_opens_the_repo(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), SEED, ViewRef::Explore);
    let view = active::<ExploreView>(cx, handle, &ws);
    cx.read(|cx| {
        assert_eq!(view.read(cx).names(), ["o/new", "o/old"]);
        assert_eq!(view.read(cx).rows.items[1].starred_by, ["bob", "ann"]);
        assert_eq!(view.read(cx).rows.selected, None);
    });
    press(cx, handle, "j");
    press(cx, handle, "j");
    press(cx, handle, "enter");
    assert_eq!(tabs(cx, &ws), ["Inbox", "Explore", "*o/old"]);
}

#[gpui_kit::test]
fn a_repos_change_reloads(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), SEED, ViewRef::Explore);
    let view = active::<ExploreView>(cx, handle, &ws);
    write(cx, "DELETE FROM following WHERE user_id = 1", &["repos"]);
    cx.read(|cx| assert_eq!(view.read(cx).names(), ["o/old"]));
}

#[gpui_kit::test]
fn no_follows_shows_the_empty_state(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = start_on(cx, dir.path(), "", ViewRef::Explore);
    let view = active::<ExploreView>(cx, handle, &ws);
    cx.read(|cx| assert!(view.read(cx).names().is_empty()));
    press(cx, handle, "j");
    press(cx, handle, "enter");
    assert_eq!(tabs(cx, &ws), ["Inbox", "*Explore"]);
}
