//! `--open-menu <name>`: show the app's dropdown menus at launch, for
//! PrintWindow screenshots of menus that otherwise need a click (no input
//! is sent to the window). Each menu comes from the same builder as its
//! button's dropdown; they are drawn side by side near the top left, not at
//! their buttons.

use std::rc::Rc;

use fl_db::queries::repo;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{App, Entity, WeakEntity, Window};

use crate::store::Store;
use crate::workspace::Workspace;

/// The names `--open-menu` takes.
pub const NAMES: [&str; 6] = ["theme", "pinned", "pinned-colour", "repo-colour", "repo-filter", "all"];

/// Build the menus `name` shows (see [`NAMES`]): "pinned-colour" is the
/// pinned menu with its "Colour" submenu and the first repo's colours open
/// beside it.
pub fn build(name: &str, ws: &Entity<Workspace>, window: &mut Window, cx: &mut App) -> Vec<Entity<PopupMenu>> {
    let pinned = Store::global(cx).read(cx).db().read(repo::pinned).unwrap_or_default();
    let first = pinned.first().map(|r| (r.id, format!("{}/{}", r.owner, r.name)));
    let entries: Vec<(String, i64)> = pinned.iter().map(|r| (format!("{}/{}", r.owner, r.name), r.id)).collect();
    let mut menus = Vec::new();
    let all = name == "all";
    if all || name == "theme" {
        menus.push(PopupMenu::build(window, cx, crate::rail::theme_menu(ws)));
    }
    if all || name == "pinned" || name == "pinned-colour" {
        menus.push(PopupMenu::build(window, cx, crate::rail::pinned_menu(ws, &pinned)));
    }
    if all || name == "pinned-colour" {
        menus.push(PopupMenu::build(window, cx, crate::rail::colour_menu(entries.clone())));
    }
    if let Some((id, full_name)) = first.filter(|_| all || name == "pinned-colour" || name == "repo-colour") {
        menus.push(PopupMenu::build(window, cx, move |menu, _, cx| crate::repo_color::menu_items(menu, id, &full_name, cx)));
    }
    if all || name == "repo-filter" {
        let builder = crate::views::queue::repo_filter_menu(Rc::new(entries), WeakEntity::new_invalid());
        menus.push(PopupMenu::build(window, cx, builder));
    }
    menus
}

#[cfg(test)]
mod tests {
    use gpui_kit::{AppContext as _, TestAppContext};

    use crate::test_support::open;

    #[gpui_kit::test]
    fn each_name_builds_its_menus(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.run_until_parked();
        // No pinned repo: "repo-colour" has no repo to show.
        let count = |cx: &mut TestAppContext, name: &str| {
            cx.update_window(handle, |_, window, cx| super::build(name, &ws, window, cx).len()).unwrap()
        };
        assert_eq!(count(cx, "theme"), 1);
        assert_eq!(count(cx, "pinned-colour"), 2);
        assert_eq!(count(cx, "repo-colour"), 0);
        assert_eq!(count(cx, "all"), 4);
    }

    #[test]
    fn an_unknown_menu_name_is_an_error() {
        let parse = |args: &[&str]| crate::Args::parse(args.iter().map(|a| a.to_string()));
        assert_eq!(parse(&["--open-menu", "theme"]).unwrap().open_menu, Some("theme"));
        assert!(parse(&["--open-menu", "nope"]).is_err());
        assert!(parse(&["--open-menu"]).is_err());
    }
}
