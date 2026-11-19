//! Repositories (every known repo but Explore-only ones) and Starred: a name
//! filter and a list of repos. `Enter` opens the repo, `p` pins it.

use std::sync::Arc;

use fl_db::queries::repo;
use fl_types::{DataChanged, PAGE_MAX, RepoFilter, RepoRow};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div, px,
};

use super::queue::FocusFilter;
use super::rows::{self, Fetch, HasRows, Rows};
use super::{Built, TogglePin};
use crate::theme::ActivePalette as _;
use crate::bench;
use crate::store::Store;
use crate::view_ref::ViewRef;

const ROW_HEIGHT: f32 = 48.;

pub struct ReposView {
    starred: bool,
    rows: Rows<RepoRow>,
    filter: String,
    input: Entity<InputState>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

fn fetch(starred: bool, query: String) -> Fetch<RepoRow> {
    let filter = RepoFilter { query, starred_only: starred };
    Arc::new(move |c, req| repo::list(c, &filter, req))
}

/// Pin or unpin a repo, then tell the views. Pinning starts its deep sync.
pub(super) fn set_pinned(repo_id: i64, pinned: bool, cx: &mut App) {
    let store = Store::global(cx).read(cx);
    let db = store.db().clone();
    let sync = store.sync().clone();
    let task = cx.background_spawn(async move { db.write(|c| repo::set_pinned(c, repo_id, pinned)) });
    cx.spawn(async move |cx| {
        if let Ok(true) = task.await {
            cx.update(|cx| Store::emit_changed(&["repos"], cx));
            if pinned {
                sync.wake();
            }
        }
    })
    .detach();
}

impl ReposView {
    pub fn build(starred: bool, window: &mut Window, cx: &mut App) -> Built {
        let view = cx.new(|cx| Self::new(starred, window, cx));
        let focus = view.read(cx).focus.clone();
        Built { view: view.into(), focus }
    }

    fn new(starred: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut rows = Rows::new(fetch(starred, String::new()), PAGE_MAX, |r: &RepoRow| r.id);
        rows.load_now(cx);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by name"));
        let subscriptions = vec![
            cx.subscribe(&Store::global(cx), |this, _, ev: &DataChanged, cx| {
                if ev.scopes.iter().any(|s| s == "repos" || s == "issues") {
                    this.rows.reload(cx);
                }
            }),
            cx.subscribe_in(&input, window, |this, input, ev: &InputEvent, window, cx| match ev {
                InputEvent::Change => {
                    this.filter = input.read(cx).value().trim().to_owned();
                    this.rows.reset(fetch(this.starred, this.filter.clone()), cx);
                }
                InputEvent::PressEnter { .. } => window.focus(&this.focus, cx),
                _ => {}
            }),
        ];
        Self { starred, rows, filter: String::new(), input, focus: cx.focus_handle(), _subscriptions: subscriptions }
    }

    fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Esc in the filter: clear it, then give the keys back to the list.
    fn escape_filter(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        if !self.filter.is_empty() {
            // `set_value` sends no `Change`.
            self.input.update(cx, |input, cx| input.set_value("", window, cx));
            self.filter.clear();
            self.rows.reset(fetch(self.starred, String::new()), cx);
        }
        window.focus(&self.focus, cx);
    }

    fn toggle_pin(&mut self, _: &TogglePin, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.selected_row() {
            set_pinned(row.id, !row.pinned, cx);
        }
    }

    fn row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.palette();
        let r = &self.rows.items[ix];
        let (id, pinned) = (r.id, r.pinned);
        let muted = theme.text_muted;
        rows::row_shell("repos-row", ix, ROW_HEIGHT, self, cx)
            .child(Icon::new(IconName::BookMarked).with_size(px(14.)).text_color(muted))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .items_center()
                            .text_sm()
                            .child(div().text_color(muted).child(format!("{} /", r.owner)))
                            .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(r.name.clone()))
                            .when(r.starred, |d| d.child(Icon::new(IconName::Star).with_size(px(12.)).text_color(theme.pending.fg)))
                            .when(r.pinned, |d| d.child(Icon::new(IconName::Pin).with_size(px(12.)).text_color(muted))),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_xs()
                            .text_color(muted)
                            .child(r.description.clone().unwrap_or_else(|| "No description.".into())),
                    ),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap(px(16.))
                    .items_center()
                    .text_xs()
                    .text_color(muted)
                    .child(format!("{} PRs \u{b7} {} issues", r.open_prs, r.open_issues))
                    .child(div().w(px(72.)).truncate().child(r.language.clone().unwrap_or_default()))
                    .child(
                        h_flex()
                            .w(px(64.))
                            .justify_end()
                            .gap(px(4.))
                            .child(Icon::new(IconName::Star).with_size(px(12.)))
                            .child(r.stars.to_string()),
                    ),
            )
            .child(
                Button::new(("repo-pin", ix))
                    .ghost()
                    .xsmall()
                    .w(px(52.))
                    .label(if pinned { "Unpin" } else { "Pin" })
                    .on_click(move |_, _, cx| set_pinned(id, !pinned, cx)),
            )
            .into_any_element()
    }

    #[cfg(test)]
    pub(super) fn names(&self) -> Vec<String> {
        self.rows.items.iter().map(|r| format!("{}/{}", r.owner, r.name)).collect()
    }

    #[cfg(test)]
    pub(super) fn input(&self) -> &Entity<InputState> {
        &self.input
    }
}

impl HasRows for ReposView {
    type Row = RepoRow;

    fn rows(&self) -> &Rows<RepoRow> {
        &self.rows
    }

    fn rows_mut(&mut self) -> &mut Rows<RepoRow> {
        &mut self.rows
    }

    fn open_ref(&self, row: &RepoRow) -> Option<ViewRef> {
        Some(ViewRef::Repo { repo_id: row.id })
    }
}

impl Render for ReposView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rows.loaded {
            bench::ready(if self.starred { "starred" } else { "repos" }, window, cx);
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
            .child(div().font_weight(FontWeight::SEMIBOLD).child(if self.starred { "Starred" } else { "Repositories" }))
            .child(div().flex_1())
            .child(
                h_flex()
                    .key_context("RepoFilter")
                    .on_action(cx.listener(Self::escape_filter))
                    .w(px(260.))
                    .gap(px(6.))
                    .items_center()
                    .child(div().text_color(theme.text_muted).child("/"))
                    .child(div().flex_1().child(Input::new(&self.input).small())),
            );
        let body = if self.rows.items.is_empty() {
            let text = if self.rows.failed {
                "Could not read the local repository cache."
            } else if !self.filter.is_empty() {
                "No repositories match."
            } else if self.starred {
                "No starred repositories yet."
            } else {
                "No repositories cached yet."
            };
            super::empty(text, cx)
        } else {
            rows::list("repos-list", cx, self, Self::row)
        };
        let root = v_flex()
            .key_context("List")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::toggle_pin))
            .size_full()
            .child(header)
            .child(body);
        rows::list_keys(root, cx)
    }
}

#[cfg(test)]
mod tests;
