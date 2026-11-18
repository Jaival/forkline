//! The icon rail on the left edge: navigation, pinned repositories, theme, settings.

use fl_types::RepoSummary;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, div, px,
};

use crate::view_ref::ViewRef;
use crate::workspace::Workspace;

pub const WIDTH: f32 = 44.;

/// `active`: the focused tab's view; its button is highlighted.
pub fn render(
    ws: &Entity<Workspace>,
    active: Option<&ViewRef>,
    inbox_unread: u32,
    pinned: &[RepoSummary],
    cx: &App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let button = |id: &'static str, icon: IconName, tip: &str, selected: bool| {
        let (bg, fg) = if selected {
            (theme.accent, theme.accent_foreground)
        } else {
            (theme.transparent, theme.muted_foreground)
        };
        let style = ButtonCustomVariant::new(cx).color(bg).foreground(fg).hover(theme.accent).active(theme.accent);
        Button::new(id).custom(style).with_size(px(26.)).rounded(px(6.)).icon(icon).tooltip(tip.to_string())
    };
    let nav = |id: &'static str, view_ref: ViewRef| {
        let ws = ws.clone();
        // A filtered Pulls tab or a Search tab with a query still lights its button.
        let selected = active.is_some_and(|a| a.same_kind(&view_ref));
        let btn = button(id, view_ref.icon(), &view_ref.title(), selected);
        btn.on_click(move |_, window, cx| ws.update(cx, |ws, cx| ws.go_to(view_ref.clone(), window, cx)))
    };

    let inbox = div()
        .relative()
        .child(nav("rail-inbox", ViewRef::Inbox))
        .when(inbox_unread > 0, |d| {
            d.child(div().absolute().top(px(2.)).right(px(2.)).size(px(6.)).rounded_full().bg(theme.primary))
        });

    let entries: Vec<(String, i64)> = pinned
        .iter()
        .map(|r| (format!("{}/{}    {} · {}", r.owner, r.name, r.open_prs, r.open_issues), r.id))
        .collect();
    let ws_for_menu = ws.clone();
    let pinned_button = button("rail-pinned", IconName::Pin, "Pinned repositories", false).dropdown_menu(
        move |mut menu, _, _| {
            if entries.is_empty() {
                return menu.item(PopupMenuItem::new("No pinned repositories").disabled(true));
            }
            for (label, repo_id) in &entries {
                let ws = ws_for_menu.clone();
                let repo_id = *repo_id;
                menu = menu.item(PopupMenuItem::new(label.clone()).on_click(move |_, window, cx| {
                    ws.update(cx, |ws, cx| ws.open(ViewRef::Repo { repo_id }, window, cx));
                }));
            }
            menu
        },
    );

    let ws_theme = ws.clone();
    let (theme_icon, theme_tip) = if theme.is_dark() { (IconName::Sun, "Light theme") } else { (IconName::Moon, "Dark theme") };
    let theme_button = button("rail-theme", theme_icon, theme_tip, false)
        .on_click(move |_, window, cx| ws_theme.update(cx, |ws, cx| ws.toggle_theme(window, cx)));

    v_flex()
        .id("rail")
        .w(px(WIDTH))
        .h_full()
        .flex_none()
        .items_center()
        .gap(px(4.))
        .py(px(8.))
        .bg(theme.sidebar)
        .border_r_1()
        .border_color(theme.border)
        .child(inbox)
        .child(nav("rail-pulls", ViewRef::Pulls { repo_id: None }))
        .child(nav("rail-issues", ViewRef::Issues { repo_id: None }))
        .child(nav("rail-reviews", ViewRef::Reviews))
        .child(nav("rail-repos", ViewRef::Repos))
        .child(nav("rail-starred", ViewRef::Starred))
        .child(nav("rail-explore", ViewRef::Explore))
        .child(nav("rail-search", ViewRef::Search { q: String::new() }))
        .child(pinned_button)
        .child(div().flex_1())
        .child(theme_button)
        .child(nav("rail-settings", ViewRef::Settings))
}
