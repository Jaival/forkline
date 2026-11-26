//! The labelled sidebar on the left edge (the default): the command box,
//! navigation with names and the inbox count, pinned repositories, then the
//! account, the theme menu and the collapse button. Ctrl+B swaps it for the
//! icon rail (`rail.rs`); both use the same `nav-*` element ids.

use fl_types::RepoSummary;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};

use crate::rail;
use crate::theme::{ActivePalette as _, SIDEBAR_WIDTH, TextScale as _};
use crate::view_ref::ViewRef;
use crate::workspace::{OpenPalette, Workspace};

/// `active`: the focused tab's view; its row is highlighted. `login`: the
/// signed-in account, shown at the bottom.
pub fn render(
    ws: &Entity<Workspace>,
    active: Option<&ViewRef>,
    inbox_unread: u32,
    pinned: &[RepoSummary],
    login: Option<&str>,
    cx: &App,
) -> impl IntoElement + use<> {
    let p = cx.palette();
    let nav = |id: &'static str, view_ref: ViewRef, count: u32| {
        let ws = ws.clone();
        // A filtered Pulls tab or a Search tab with a query still lights its row.
        let selected = active.is_some_and(|a| a.same_kind(&view_ref));
        // Selected: the accent-tinted row colour; hover: the neutral raised one.
        let (bg, fg, icon_fg) = if selected { (p.selected, p.text_strong, p.accent_text) } else { (p.surface, p.text_muted, p.text_muted) };
        let count = (count > 0).then(|| {
            let text = if count > 99 { "99+".to_owned() } else { count.to_string() };
            div().text_small().text_color(if selected { p.accent_text } else { p.text_muted }).child(text)
        });
        h_flex()
            .id(id)
            .test_support()
            .h(px(30.))
            .px(px(10.))
            .gap(px(10.))
            .items_center()
            .rounded(px(6.))
            .bg(bg)
            .text_color(fg)
            .cursor_pointer()
            .hover(move |s| s.bg(p.raised))
            .child(Icon::new(view_ref.icon()).with_size(px(16.)).text_color(icon_fg))
            .child(div().flex_1().min_w_0().truncate().child(view_ref.title()))
            .children(count)
            .on_click(move |_, window, cx| ws.update(cx, |ws, cx| ws.go_to(view_ref.clone(), window, cx)))
    };
    let heading = |text: &'static str| {
        h_flex()
            .h(px(28.))
            .mt(px(10.))
            .px(px(10.))
            .items_center()
            .text_label()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(p.text_faint)
            .child(div().flex_1().child(text))
    };
    let icon_button = |id: &'static str, icon: IconName, tip: &str| {
        let style = ButtonCustomVariant::new(cx).color(p.surface).foreground(p.text_muted).hover(p.raised).active(p.raised);
        Button::new(id).custom(style).small().icon(Icon::new(icon).with_size(px(16.))).tooltip(tip.to_string())
    };

    let command_box = h_flex()
        .id("sidebar-command")
        .test_support()
        .h(px(30.))
        .px(px(10.))
        .mb(px(6.))
        .gap(px(8.))
        .items_center()
        .rounded(px(6.))
        .border_1()
        .border_color(p.border)
        .text_small()
        .text_color(p.text_muted)
        .cursor_pointer()
        .hover(move |s| s.bg(p.raised))
        .child(Icon::new(IconName::Search).with_size(px(14.)))
        .child(div().flex_1().child("Commands"))
        .child(div().text_color(p.text_faint).child("Ctrl K"))
        .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenPalette), cx));

    let pinned_rows = pinned.iter().map(|r| {
        let name = format!("{}/{}", r.owner, r.name);
        let ws = ws.clone();
        let repo_id = r.id;
        let counts = format!("{} · {}", r.open_prs, r.open_issues);
        h_flex()
            .id(SharedString::from(format!("nav-pinned-{repo_id}")))
            .test_support()
            .h(px(28.))
            .px(px(10.))
            .gap(px(10.))
            .items_center()
            .rounded(px(6.))
            .text_color(p.text_muted)
            .cursor_pointer()
            .hover(move |s| s.bg(p.raised))
            .child(crate::repo_color::dot(&name, cx))
            // The owner is often the same on every row: show the name, the
            // full name on hover.
            .child(div().flex_1().min_w_0().truncate().child(r.name.clone()))
            .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name.clone()).build(window, cx))
            .child(div().text_small().text_color(p.text_faint).child(counts))
            .on_click(move |_, window, cx| ws.update(cx, |ws, cx| ws.open(ViewRef::Repo { repo_id }, window, cx)))
    });
    let colours: Vec<(String, i64)> = pinned.iter().map(|r| (format!("{}/{}", r.owner, r.name), r.id)).collect();
    let pinned_heading = heading("Pinned").when(!colours.is_empty(), |h| {
        h.child(icon_button("nav-pinned-colours", IconName::Palette, "Repository colours").dropdown_menu(rail::colour_menu(colours)))
    });

    let account = login.map(|login| {
        let style = ButtonCustomVariant::new(cx).color(p.surface).foreground(p.text).hover(p.raised).active(p.raised);
        Button::new("nav-account")
            .custom(style)
            .flex_1()
            .min_w_0()
            .justify_start()
            .child(
                h_flex()
                    .min_w_0()
                    .gap(px(8.))
                    .items_center()
                    .child(rail::avatar(login, 26., p))
                    .child(div().min_w_0().truncate().child(login.to_owned())),
            )
            .dropdown_menu(rail::account_menu(ws, login))
    });
    let theme_icon = if p.dark { IconName::Moon } else { IconName::Sun };
    let collapse = {
        let ws = ws.clone();
        icon_button("nav-collapse", IconName::PanelLeftClose, "Collapse sidebar (Ctrl+B)")
            .on_click(move |_, window, cx| ws.update(cx, |ws, cx| ws.toggle_sidebar(window, cx)))
    };

    v_flex()
        .id("sidebar")
        .w(px(SIDEBAR_WIDTH))
        .h_full()
        .flex_none()
        .p(px(8.))
        .bg(p.surface)
        .text_body()
        .child(command_box)
        .child(nav("nav-inbox", ViewRef::Inbox, inbox_unread))
        .child(nav("nav-pulls", ViewRef::Pulls { repo_id: None }, 0))
        .child(nav("nav-issues", ViewRef::Issues { repo_id: None }, 0))
        .child(nav("nav-reviews", ViewRef::Reviews, 0))
        .child(heading("Browse"))
        .child(nav("nav-repos", ViewRef::Repos, 0))
        .child(nav("nav-starred", ViewRef::Starred, 0))
        .child(nav("nav-explore", ViewRef::Explore, 0))
        .child(nav("nav-search", ViewRef::Search { q: String::new() }, 0))
        .child(pinned_heading)
        .child(
            v_flex()
                .id("nav-pinned-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(pinned_rows)
                .when(pinned.is_empty(), |list| {
                    // Pinned repos get every issue and PR synced; without
                    // any, the lists show only the viewer's own items.
                    let ws = ws.clone();
                    list.child(
                        v_flex()
                            .px(px(10.))
                            .gap(px(6.))
                            .items_start()
                            .child(div().text_small().text_color(p.text_faint).child("Pin repositories to sync all their issues and pull requests."))
                            .child(
                                Button::new("nav-pin-repos")
                                    .small()
                                    .outline()
                                    .icon(Icon::new(IconName::Pin).with_size(px(14.)))
                                    .label("Pin repositories")
                                    .on_click(move |_, window, cx| ws.update(cx, |ws, cx| ws.go_to(ViewRef::Repos, window, cx))),
                            ),
                    )
                }),
        )
        .child(
            h_flex()
                .pt(px(6.))
                .px(px(4.))
                .gap(px(4.))
                .items_center()
                .children(account)
                .when(login.is_none(), |h| h.child(div().flex_1()))
                .child(icon_button("nav-theme", theme_icon, "Theme").dropdown_menu(rail::theme_menu(ws)))
                .child(collapse),
        )
}
