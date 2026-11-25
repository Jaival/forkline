//! The icon rail: the collapsed sidebar (`sidebar.rs`, Ctrl+B). Navigation,
//! pinned repositories, the expand button, the theme menu and the account
//! avatar.

use fl_types::RepoSummary;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{Sizable as _, ThemeMode, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Context, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Styled as _, Window, div, px,
};

use crate::theme::{ActivePalette as _, Choices, Palette, RAIL_BUTTON, RAIL_WIDTH};
use crate::view_ref::ViewRef;
use crate::workspace::Workspace;

/// `active`: the focused tab's view; its button is highlighted. `login`: the
/// signed-in account, shown as an avatar at the bottom.
pub fn render(
    ws: &Entity<Workspace>,
    active: Option<&ViewRef>,
    inbox_unread: u32,
    pinned: &[RepoSummary],
    login: Option<&str>,
    cx: &App,
) -> impl IntoElement + use<> {
    let p = cx.palette();
    let button = |id: &'static str, icon: IconName, tip: &str, selected: bool| {
        let (bg, fg) = if selected { (p.selected, p.accent_text) } else { (p.surface.opacity(0.), p.text_muted) };
        let style = ButtonCustomVariant::new(cx).color(bg).foreground(fg).hover(p.raised).active(p.raised);
        Button::new(id)
            .custom(style)
            .with_size(px(RAIL_BUTTON))
            .rounded(px(8.))
            .icon(gpui_kit::component::Icon::new(icon).with_size(px(18.)))
            .tooltip(tip.to_string())
    };
    let nav = |id: &'static str, view_ref: ViewRef| {
        let ws = ws.clone();
        // A filtered Pulls tab or a Search tab with a query still lights its button.
        let selected = active.is_some_and(|a| a.same_kind(&view_ref));
        let btn = button(id, view_ref.icon(), &view_ref.title(), selected);
        btn.on_click(move |_, window, cx| ws.update(cx, |ws, cx| ws.go_to(view_ref.clone(), window, cx)))
    };

    let inbox = div().relative().child(nav("nav-inbox", ViewRef::Inbox)).when(inbox_unread > 0, |d| {
        let count = if inbox_unread > 99 { "99+".to_owned() } else { inbox_unread.to_string() };
        d.child(
            div()
                .id("nav-inbox-count")
                .absolute()
                .top(px(1.))
                .right(px(0.))
                .min_w(px(15.))
                .h(px(15.))
                .px(px(3.))
                .rounded_full()
                .border_2()
                .border_color(p.surface)
                .bg(p.accent_fill)
                .text_color(p.on_accent)
                .text_size(px(9.))
                .line_height(px(11.))
                .font_weight(FontWeight::BOLD)
                .flex()
                .items_center()
                .justify_center()
                .child(count),
        )
    });

    let pinned_button =
        button("nav-pinned", IconName::Pin, "Pinned repositories", false).dropdown_menu(pinned_menu(ws, pinned));
    let expand = {
        let ws = ws.clone();
        button("nav-expand", IconName::PanelLeftOpen, "Expand sidebar (Ctrl+B)", false)
            .on_click(move |_, window, cx| ws.update(cx, |ws, cx| ws.toggle_sidebar(window, cx)))
    };
    let theme_icon = if p.dark { IconName::Moon } else { IconName::Sun };
    let theme_button = button("nav-theme", theme_icon, "Theme", false).dropdown_menu(theme_menu(ws));

    let account = login.map(|login| {
        let style = ButtonCustomVariant::new(cx).color(p.surface.opacity(0.)).foreground(p.text).hover(p.raised).active(p.raised);
        Button::new("nav-account")
            .custom(style)
            .with_size(px(RAIL_BUTTON))
            .rounded(px(8.))
            .mt(px(4.))
            .child(avatar(login, 28., p))
            .tooltip(format!("Signed in as {login}"))
            .dropdown_menu(account_menu(ws, login))
    });

    v_flex()
        .id("rail")
        .w(px(RAIL_WIDTH))
        .h_full()
        .flex_none()
        .items_center()
        .gap(px(4.))
        .pt(px(6.))
        .pb(px(10.))
        .bg(p.surface)
        .child(inbox)
        .child(nav("nav-pulls", ViewRef::Pulls { repo_id: None }))
        .child(nav("nav-issues", ViewRef::Issues { repo_id: None }))
        .child(nav("nav-reviews", ViewRef::Reviews))
        .child(div().w(px(20.)).h(px(1.)).my(px(4.)).bg(p.border))
        .child(nav("nav-repos", ViewRef::Repos))
        .child(nav("nav-starred", ViewRef::Starred))
        .child(nav("nav-explore", ViewRef::Explore))
        .child(nav("nav-search", ViewRef::Search { q: String::new() }))
        .child(pinned_button)
        .child(div().flex_1())
        .child(expand)
        .child(theme_button)
        .children(account)
}

/// The account menu (sidebar and rail): who is signed in, and Sign out. A
/// kit menu, so it closes on an outside click and on Esc.
pub fn account_menu(
    ws: &Entity<Workspace>,
    login: &str,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    let ws = ws.clone();
    let title = format!("Signed in as {login}");
    move |menu, _, cx| {
        let menu = menu.label(title.clone());
        if ws.read(cx).auth(cx).is_some_and(|a| a.env_override) {
            return menu.label("Set by FORKLINE_GITHUB_TOKEN. Unset it to sign out.");
        }
        let ws = ws.clone();
        menu.separator()
            .item(PopupMenuItem::new("Sign out").on_click(move |_, _, cx| ws.update(cx, |ws, cx| ws.sign_out(cx))))
    }
}

/// A round avatar of `size` px: the login's initials on its hash colour.
pub fn avatar(login: &str, size: f32, p: &Palette) -> gpui_kit::Div {
    let (bg, fg) = p.avatar(login);
    div()
        .size(px(size))
        .flex_none()
        .rounded_full()
        .bg(bg)
        .text_color(fg)
        .text_size(px((size * 0.4).round().max(8.)))
        .line_height(px(size))
        .font_weight(if size < 20. { FontWeight::BOLD } else { FontWeight::SEMIBOLD })
        .flex()
        .justify_center()
        .child(initials(login))
}

/// Up to two capitals from the words of a login (`lena-dev` -> `LD`).
pub fn initials(login: &str) -> String {
    let words = login.split(|c: char| !c.is_ascii_alphabetic()).filter(|w| !w.is_empty());
    let mut out: String = words.filter_map(|w| w.chars().next()).take(2).collect();
    if out.is_empty() {
        out = login.chars().take(1).collect();
    }
    out.to_uppercase()
}

/// The pinned-repositories menu: each pinned repo, then the "Colour"
/// submenu. Also built by `--open-menu` (see `menu_preview.rs`).
pub fn pinned_menu(
    ws: &Entity<Workspace>,
    pinned: &[RepoSummary],
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    // (owner/name, counts, id)
    let entries: Vec<(String, String, i64)> = pinned
        .iter()
        .map(|r| (format!("{}/{}", r.owner, r.name), format!("{} · {}", r.open_prs, r.open_issues), r.id))
        .collect();
    let ws_for_menu = ws.clone();
    move |mut menu, window, cx| {
        if entries.is_empty() {
            return menu.item(PopupMenuItem::new("No pinned repositories").disabled(true));
        }
        for (name, counts, repo_id) in &entries {
            let ws = ws_for_menu.clone();
            let repo_id = *repo_id;
            let (name, counts) = (name.clone(), counts.clone());
            menu = menu.item(
                PopupMenuItem::element(move |_, cx| {
                    h_flex()
                        .gap(px(8.))
                        .items_center()
                        .child(crate::repo_color::dot(&name, cx))
                        .child(name.clone())
                        .child(div().text_color(cx.palette().text_muted).child(counts.clone()))
                })
                .on_click(move |_, window, cx| {
                    ws.update(cx, |ws, cx| ws.open(ViewRef::Repo { repo_id }, window, cx));
                }),
            );
        }
        let repos = entries.iter().map(|(name, _, id)| (name.clone(), *id)).collect();
        menu.separator().submenu("Colour", window, cx, colour_menu(repos))
    }
}

/// The pinned menu's "Colour" submenu: one submenu of colours per repo
/// (`owner/name`, id).
pub fn colour_menu(repos: Vec<(String, i64)>) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |mut menu, window, cx| {
        for (name, repo_id) in &repos {
            let (name, repo_id) = (name.clone(), *repo_id);
            menu = menu.submenu(name.clone(), window, cx, move |menu, _, cx| {
                crate::repo_color::menu_items(menu, repo_id, &name, cx)
            });
        }
        menu
    }
}

/// The theme menu: System / Light / Dark (the saved mode checked), then
/// "Appearance…".
pub fn theme_menu(ws: &Entity<Workspace>) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    let ws_theme = ws.clone();
    move |menu, _, cx| {
        let mode = cx.try_global::<Choices>().and_then(|c| c.mode);
        let item = |label: &'static str, pick: Option<ThemeMode>| {
            let ws = ws_theme.clone();
            PopupMenuItem::new(label)
                .checked(mode == pick)
                .on_click(move |_, window, cx| ws.update(cx, |ws, cx| ws.set_mode(pick, window, cx)))
        };
        let ws = ws_theme.clone();
        menu.item(item("System", None))
            .item(item("Light", Some(ThemeMode::Light)))
            .item(item("Dark", Some(ThemeMode::Dark)))
            .separator()
            .item(
                PopupMenuItem::new("Appearance…")
                    .on_click(move |_, window, cx| ws.update(cx, |ws, cx| ws.open_appearance(window, cx))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::initials;

    #[test]
    fn initials_take_the_first_letter_of_each_word() {
        assert_eq!(initials("lena-dev"), "LD");
        assert_eq!(initials("jaival"), "J");
        assert_eq!(initials("zed-industries"), "ZI");
        assert_eq!(initials("a-b-c"), "AB");
        assert_eq!(initials("42"), "4");
        assert_eq!(initials(""), "");
    }
}
