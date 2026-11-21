//! Home: open issues and PRs across the synced repos, newest update first.
//! `Enter` opens the item.

use std::sync::Arc;

use fl_db::queries::lists;
use fl_types::{DataChanged, IssueRow, IssueState, PAGE_MAX};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, FocusHandle, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div, px,
};

use super::Built;
use super::queue::age;
use super::rows::{self, HasRows, Rows};
use crate::theme::ActivePalette as _;
use crate::bench;
use crate::store::Store;
use crate::view_ref::ViewRef;
use crate::theme::TextScale as _;

const ROW_HEIGHT: f32 = 46.;

/// State icon and colour of an issue or PR.
pub(super) fn state_icon(row: &IssueRow, theme: &crate::theme::Palette) -> (IconName, Hsla) {
    match (row.is_pr, row.state) {
        (true, IssueState::Open) => (IconName::GitPullRequest, theme.open.fg),
        (true, IssueState::Merged) => (IconName::GitMerge, theme.merged.fg),
        (true, IssueState::Closed) => (IconName::GitPullRequestClosed, theme.closed.fg),
        (false, IssueState::Open) => (IconName::CircleDot, theme.open.fg),
        (false, _) => (IconName::CircleCheck, theme.merged.fg),
    }
}

/// The tab an issue row opens.
pub(super) fn item_ref(row: &IssueRow) -> ViewRef {
    if row.is_pr { ViewRef::Pr { id: row.id } } else { ViewRef::Issue { id: row.id } }
}

pub struct HomeView {
    rows: Rows<IssueRow>,
    focus: FocusHandle,
    _store: Subscription,
}

impl HomeView {
    pub fn build(_: &mut Window, cx: &mut App) -> Built {
        let view = cx.new(|cx| {
            let mut rows = Rows::new(Arc::new(lists::home_feed), PAGE_MAX, |r: &IssueRow| r.id);
            rows.load_now(cx);
            let _store = cx.subscribe(&Store::global(cx), |this: &mut Self, _, ev: &DataChanged, cx| {
                if ev.scopes.iter().any(|s| s == "issues" || s == "pr") {
                    this.rows.reload(cx);
                }
            });
            Self { rows, focus: cx.focus_handle(), _store }
        });
        let focus = view.read(cx).focus.clone();
        Built { view: view.into(), focus }
    }

    fn row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.palette();
        let r = &self.rows.items[ix];
        let (icon, color) = state_icon(r, &theme);
        let comments = match r.comments {
            1 => " \u{b7} 1 comment".to_owned(),
            0 => String::new(),
            n => format!(" \u{b7} {n} comments"),
        };
        rows::row_shell("home-row", ix, ROW_HEIGHT, self, cx)
            .child(Icon::new(icon).with_size(px(14.)).text_color(color))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_body()
                            .when(r.unread, |d| d.font_weight(FontWeight::SEMIBOLD))
                            .child(r.title.clone()),
                    )
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .items_center()
                            .text_small()
                            .text_color(theme.text_muted)
                            .child(crate::repo_color::dot(&r.repo, cx))
                            .child(div().min_w_0().truncate().child(format!("{} #{} \u{b7} {}{comments}", r.repo, r.number, r.author))),
                    ),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap(px(6.))
                    .items_center()
                    .text_small()
                    .text_color(theme.text_muted)
                    .when(r.unread, |d| d.child(div().size(px(6.)).rounded_full().bg(theme.accent)))
                    .child(age(fl_sync::now(), r.updated_at)),
            )
            .into_any_element()
    }

    #[cfg(test)]
    pub(super) fn ids(&self) -> Vec<i64> {
        self.rows.items.iter().map(|r| r.id).collect()
    }

    #[cfg(test)]
    pub(super) fn selected_id(&self) -> Option<i64> {
        self.rows.selected_row().map(|r| r.id)
    }
}

impl HasRows for HomeView {
    type Row = IssueRow;

    fn rows(&self) -> &Rows<IssueRow> {
        &self.rows
    }

    fn rows_mut(&mut self) -> &mut Rows<IssueRow> {
        &mut self.rows
    }

    fn open_ref(&self, row: &IssueRow) -> Option<ViewRef> {
        Some(item_ref(row))
    }
}

impl Render for HomeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rows.loaded {
            bench::ready("home", window, cx);
        }
        let theme = *cx.palette();
        let header = h_flex()
            .flex_none()
            .h(px(36.))
            .px(px(12.))
            .gap(px(8.))
            .items_center()
            .border_b_1()
            .border_color(theme.border)
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Home"))
            .child(div().text_small().text_color(theme.text_muted).child("Recent activity"));
        let body = if self.rows.items.is_empty() {
            super::empty(
                if self.rows.failed { "Could not read the local activity cache." } else { "No cached activity yet." },
                cx,
            )
        } else {
            rows::list("home-list", cx, self, Self::row)
        };
        let root = v_flex().key_context("List").track_focus(&self.focus).size_full().child(header).child(body);
        rows::list_keys(root, cx)
    }
}

#[cfg(test)]
mod tests;
