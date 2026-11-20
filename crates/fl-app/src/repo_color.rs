//! Repository colours in views: the dot before a repo name and the "Colour"
//! menu that picks one. Hues and per-theme tuning live in `theme.rs`
//! ([`REPO_HUES`], [`RepoColors`]); the pick is stored in `repos.color`.

use fl_db::queries::repo;
use gpui_kit::component::h_flex;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{App, AppContext as _, Div, ParentElement as _, Styled as _, div, px};

use crate::store::Store;
use crate::theme::{ActivePalette as _, REPO_HUES, RepoColors, repo_color};

/// The 8 px dot of the repository `owner/name`.
pub fn dot(full_name: &str, cx: &App) -> Div {
    div().size(px(8.)).flex_none().rounded_full().bg(repo_color(full_name, cx))
}

/// Pick colour `index` (`None`: back to the one from the name) for a repo:
/// saved in the background, shown at once in every window.
pub fn set(repo_id: i64, full_name: &str, index: Option<u8>, cx: &mut App) {
    if !cx.has_global::<RepoColors>() {
        cx.set_global(RepoColors::default());
    }
    cx.global_mut::<RepoColors>().set(full_name, index);
    let db = Store::global(cx).read(cx).db().clone();
    cx.background_spawn(async move {
        let _ = db.write(|c| repo::set_color(c, repo_id, index));
    })
    .detach();
    cx.refresh_windows();
}

/// "Automatic" plus the ten colours, the repo's current pick checked.
pub fn menu_items(mut menu: PopupMenu, repo_id: i64, full_name: &str, cx: &App) -> PopupMenu {
    let picked = cx.try_global::<RepoColors>().and_then(|c| c.picked(full_name));
    let auto_name = full_name.to_owned();
    menu = menu.item(
        PopupMenuItem::new("Automatic")
            .checked(picked.is_none())
            .on_click(move |_, _, cx| set(repo_id, &auto_name, None, cx)),
    );
    for (i, (_, name)) in REPO_HUES.iter().enumerate() {
        let i = i as u8;
        let full_name = full_name.to_owned();
        let label = *name;
        menu = menu.item(
            PopupMenuItem::element(move |_, cx| {
                let c = cx.palette().repo(i);
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(div().size(px(10.)).rounded_full().bg(c))
                    .child(label)
            })
            .checked(picked == Some(i))
            .on_click(move |_, _, cx| set(repo_id, &full_name, Some(i), cx)),
        );
    }
    menu
}
