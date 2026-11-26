//! Explore: repos the people you follow starred lately, newest star first.
//! `Enter` opens the repo; `p` or the row button pins it (and so syncs it).

use std::sync::Arc;

use fl_db::queries::explore;
use fl_types::{DataChanged, ExploreRow, PAGE_MAX};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, Subscription, Window, div, px,
};

use super::repos::set_pinned;
use super::{Built, TogglePin};
use super::queue::age;
use super::rows::{self, HasRows, Rows};
use crate::theme::ActivePalette as _;
use crate::bench;
use crate::store::Store;
use crate::view_ref::ViewRef;
use crate::theme::{TextScale as _, scaled};

const ROW_HEIGHT: f32 = 62.;

pub struct ExploreView {
    rows: Rows<ExploreRow>,
    focus: FocusHandle,
    _store: Subscription,
}

/// "starred by ann, bob and 3 others".
fn starred_by(row: &ExploreRow) -> String {
    let names = row.starred_by.join(", ");
    match row.starred_by_count.saturating_sub(row.starred_by.len() as u32) {
        0 => format!("starred by {names}"),
        1 => format!("starred by {names} and 1 other"),
        n => format!("starred by {names} and {n} others"),
    }
}

impl ExploreView {
    pub fn build(_: &mut Window, cx: &mut App) -> Built {
        let view = cx.new(|cx| {
            let mut rows = Rows::new(Arc::new(explore::feed), PAGE_MAX, |r: &ExploreRow| r.repo_id);
            rows.load_now(cx);
            let _store = cx.subscribe(&Store::global(cx), |this: &mut Self, _, ev: &DataChanged, cx| {
                if ev.scopes.iter().any(|s| s == "repos") {
                    this.rows.reload(cx);
                }
            });
            Self { rows, focus: cx.focus_handle(), _store }
        });
        let focus = view.read(cx).focus.clone();
        Built { view: view.into(), focus }
    }

    fn toggle_pin(&mut self, _: &TogglePin, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.selected_row() {
            set_pinned(row.repo_id, !row.pinned, cx);
        }
    }

    fn row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.palette();
        let r = &self.rows.items[ix];
        let (id, pinned) = (r.repo_id, r.pinned);
        let muted = theme.text_muted;
        rows::row_shell("explore-row", ix, ROW_HEIGHT, self, cx)
            .child(Icon::new(IconName::BookMarked).with_size(px(14.)).text_color(muted))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .items_center()
                            .text_body()
                            .child(div().text_color(muted).child(format!("{} /", r.owner)))
                            .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(r.name.clone()))
                            .when(r.starred, |d| {
                                d.child(Icon::new(IconName::Star).with_size(px(12.)).text_color(theme.pending.fg))
                            })
                            .when(pinned, |d| d.child(Icon::new(IconName::Pin).with_size(px(12.)).text_color(muted))),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_small()
                            .text_color(muted)
                            .child(r.description.clone().unwrap_or_else(|| "No description.".into())),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_small()
                            .text_color(muted)
                            .child(format!("{} \u{b7} {}", starred_by(r), age(fl_sync::now(), r.last_starred_at))),
                    ),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap(px(16.))
                    .items_center()
                    .text_small()
                    .text_color(muted)
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
                Button::new(("explore-pin", ix))
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
}

impl HasRows for ExploreView {
    type Row = ExploreRow;

    fn rows(&self) -> &Rows<ExploreRow> {
        &self.rows
    }

    fn rows_mut(&mut self) -> &mut Rows<ExploreRow> {
        &mut self.rows
    }

    fn open_ref(&self, row: &ExploreRow) -> Option<ViewRef> {
        Some(ViewRef::Repo { repo_id: row.repo_id })
    }
}

impl Render for ExploreView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rows.loaded {
            bench::ready("explore", window, cx);
        }
        let theme = *cx.palette();
        let header = h_flex()
            .flex_none()
            .h(scaled(36.))
            .px(px(12.))
            .gap(px(8.))
            .items_center()
            .border_b_1()
            .border_color(theme.border)
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Explore"))
            .child(div().text_small().text_color(theme.text_muted).child("Recent stars from people you follow"));
        let body = if self.rows.items.is_empty() {
            super::empty(
                if self.rows.failed {
                    "Could not read the local Explore cache."
                } else {
                    "Follow people on GitHub to see what they star here."
                },
                cx,
            )
        } else {
            rows::list("explore-list", cx, self, Self::row)
        };
        let root = v_flex()
            .key_context("List")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::toggle_pin))
            .size_full()
            .child(header)
            .child(body);
        rows::list_keys(root, cx)
    }
}

#[cfg(test)]
mod tests;
