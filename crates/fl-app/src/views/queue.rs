//! The work queue: one grouped, virtualized list behind Inbox, Pull requests,
//! Issues and Review requests.

use std::rc::Rc;

use fl_db::queries::{queue, repo};
use fl_types::{
    CheckState, DataChanged, IssueState, PAGE_MAX, PageReq, QueueGroup, QueueKind, QueueQuery, QueueRow,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::VirtualListScrollHandle;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, Theme, h_flex, v_flex, v_virtual_list};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, EventEmitter, FocusHandle, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, ScrollStrategy, SharedString,
    Size, StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window, actions,
    div, px, size,
};
use rusqlite::Connection;

use crate::bench;
use crate::store::Store;
use crate::view_ref::ViewRef;
use crate::workspace::OpenView;

actions!(queue, [SelectNext, SelectPrev, OpenSelected]);

const HEADER_HEIGHT: f32 = 26.;
const ROW_HEIGHT: f32 = 46.;
/// Start the next page when the rendered range ends this close to the end.
const LOAD_AHEAD: usize = 20;

pub enum QueueEvent {
    Selected(Option<QueueRow>),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Item {
    Header(QueueGroup),
    /// Index into `Queue::rows`.
    Row(usize),
}

/// One header before the first row of each group. Rows arrive ordered by group.
fn flatten(rows: &[QueueRow]) -> Vec<Item> {
    let mut items = Vec::with_capacity(rows.len() + QueueGroup::ALL.len());
    let mut group = None;
    for (ix, row) in rows.iter().enumerate() {
        if group != Some(row.group) {
            group = Some(row.group);
            items.push(Item::Header(row.group));
        }
        items.push(Item::Row(ix));
    }
    items
}

/// `now`, `5m`, `2h`, `3d`, `4w`, `2y`.
pub(super) fn age(now: i64, then: i64) -> String {
    match now - then {
        ..60 => "now".into(),
        s @ 60..3_600 => format!("{}m", s / 60),
        s @ 3_600..86_400 => format!("{}h", s / 3_600),
        s @ 86_400..604_800 => format!("{}d", s / 86_400),
        s @ 604_800..31_536_000 => format!("{}w", s / 604_800),
        s => format!("{}y", s / 31_536_000),
    }
}

struct Snapshot {
    rows: Vec<QueueRow>,
    next: Option<u32>,
    counts: Vec<(QueueGroup, u32)>,
}

/// The first `pages` pages and the group counts.
fn snapshot(conn: &Connection, query: &QueueQuery, pages: usize) -> rusqlite::Result<Snapshot> {
    let mut rows = Vec::new();
    let mut offset = 0;
    let mut next = None;
    for _ in 0..pages {
        let page = queue::page(conn, query, PageReq { offset, limit: PAGE_MAX })?;
        rows.extend(page.items);
        next = page.next;
        match next {
            Some(n) => offset = n,
            None => break,
        }
    }
    Ok(Snapshot { rows, next, counts: queue::counts(conn, query)? })
}

pub struct Queue {
    query: QueueQuery,
    title: SharedString,
    rows: Vec<QueueRow>,
    counts: Vec<(QueueGroup, u32)>,
    next: Option<u32>,
    loading: bool,
    /// Bumped when a read in flight turns stale (reload, filter change).
    generation: u32,
    /// Index into `rows`.
    selected: Option<usize>,
    items: Vec<Item>,
    sizes: Rc<Vec<Size<Pixels>>>,
    scroll: VirtualListScrollHandle,
    pub focus: FocusHandle,
    /// `(owner/name, id)` of the pinned repositories, for the filter.
    repos: Rc<Vec<(String, i64)>>,
    _store: Subscription,
}

impl EventEmitter<QueueEvent> for Queue {}

impl Queue {
    pub fn new(query: QueueQuery, title: SharedString, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Read synchronously: the first frame must have rows.
        let db = Store::global(cx).read(cx).db().clone();
        let snap = db.read(|c| snapshot(c, &query, 1)).ok();
        let repos = db
            .read(repo::pinned)
            .map(|pinned| pinned.into_iter().map(|r| (format!("{}/{}", r.owner, r.name), r.id)).collect())
            .unwrap_or_default();
        let subscription = cx.subscribe(&Store::global(cx), |this, _, ev: &DataChanged, cx| {
            if ev.scopes.iter().any(|s| matches!(s.as_str(), "inbox" | "issues" | "pr")) {
                this.reload(cx);
            }
        });
        let mut this = Self {
            query,
            title,
            rows: Vec::new(),
            counts: Vec::new(),
            next: None,
            loading: false,
            generation: 0,
            selected: None,
            items: Vec::new(),
            sizes: Rc::default(),
            scroll: VirtualListScrollHandle::new(),
            focus: cx.focus_handle(),
            repos: Rc::new(repos),
            _store: subscription,
        };
        if let Some(snap) = snap {
            this.rows = snap.rows;
            this.next = snap.next;
            this.counts = snap.counts;
            this.rebuild();
        }
        this
    }

    fn rebuild(&mut self) {
        self.items = flatten(&self.rows);
        let sizes = self.items.iter().map(|item| {
            let height = if matches!(item, Item::Header(_)) { HEADER_HEIGHT } else { ROW_HEIGHT };
            size(px(0.), px(height))
        });
        self.sizes = Rc::new(sizes.collect());
    }

    fn select(&mut self, selected: Option<usize>, cx: &mut Context<Self>) {
        if self.selected == selected {
            return;
        }
        self.selected = selected;
        cx.emit(QueueEvent::Selected(selected.map(|ix| self.rows[ix].clone())));
        cx.notify();
    }

    fn move_selection(&mut self, step: isize, cx: &mut Context<Self>) {
        let Some(last) = self.rows.len().checked_sub(1) else { return };
        let ix = match self.selected {
            None => 0,
            Some(ix) => ix.saturating_add_signed(step).min(last),
        };
        self.select(Some(ix), cx);
        if let Some(item_ix) = self.items.iter().position(|item| *item == Item::Row(ix)) {
            self.scroll.scroll_to_item(item_ix, ScrollStrategy::Nearest);
        }
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(1, cx);
    }

    fn select_prev(&mut self, _: &SelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(-1, cx);
    }

    fn open_selected(&mut self, _: &OpenSelected, window: &mut Window, cx: &mut Context<Self>) {
        self.open(window, cx);
    }

    fn open(&self, window: &mut Window, cx: &mut App) {
        let Some(row) = self.selected.map(|ix| &self.rows[ix]) else { return };
        let Some(id) = row.issue_id else { return };
        let view = if row.is_pr { ViewRef::Pr { id } } else { ViewRef::Issue { id } };
        window.dispatch_action(Box::new(OpenView(view)), cx);
    }

    fn set_repo(&mut self, repo_id: Option<i64>, cx: &mut Context<Self>) {
        self.query.repo_id = repo_id;
        self.generation += 1;
        self.loading = false;
        self.selected = None;
        let query = self.query;
        let snap = Store::global(cx).read(cx).db().read(|c| snapshot(c, &query, 1)).ok();
        self.rows = Vec::new();
        self.counts = Vec::new();
        self.next = None;
        if let Some(snap) = snap {
            self.rows = snap.rows;
            self.next = snap.next;
            self.counts = snap.counts;
        }
        self.rebuild();
        cx.emit(QueueEvent::Selected(None));
        cx.notify();
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        let Some(offset) = self.next else { return };
        if self.loading {
            return;
        }
        self.loading = true;
        let generation = self.generation;
        let query = self.query;
        let task = Store::global(cx).read(cx).read(cx, move |c| {
            queue::page(c, &query, PageReq { offset, limit: PAGE_MAX })
        });
        cx.spawn(async move |this, cx| {
            let page = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                if let Ok(page) = page {
                    this.rows.extend(page.items);
                    this.next = page.next;
                    this.rebuild();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Read again as many pages as are loaded now; keep the selection by key.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        self.loading = true;
        let generation = self.generation;
        let query = self.query;
        let pages = self.rows.len().div_ceil(PAGE_MAX as usize).max(1);
        let task = Store::global(cx).read(cx).read(cx, move |c| snapshot(c, &query, pages));
        cx.spawn(async move |this, cx| {
            let snap = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                if let Ok(snap) = snap {
                    this.apply(snap, cx);
                }
            });
        })
        .detach();
    }

    fn apply(&mut self, snap: Snapshot, cx: &mut Context<Self>) {
        let before_ix = self.selected;
        let before = before_ix.map(|ix| self.rows[ix].clone());
        self.rows = snap.rows;
        self.next = snap.next;
        self.counts = snap.counts;
        // A selected row that left (Done, Snooze, sync) passes the selection
        // to the row that took its place, so triage goes on with the next one.
        self.selected = before.as_ref().and_then(|b| {
            self.rows.iter().position(|r| r.key == b.key).or_else(|| {
                let last = self.rows.len().checked_sub(1)?;
                before_ix.map(|ix| ix.min(last))
            })
        });
        self.rebuild();
        let after = self.selected.map(|ix| &self.rows[ix]);
        if after != before.as_ref() {
            cx.emit(QueueEvent::Selected(after.cloned()));
        }
        cx.notify();
    }

    fn header_element(&self, group: QueueGroup, theme: &Theme) -> AnyElement {
        let count = self.counts.iter().find(|(g, _)| *g == group).map_or(0, |(_, n)| *n);
        h_flex()
            .w_full()
            .h(px(HEADER_HEIGHT))
            .px(px(10.))
            .items_center()
            .justify_between()
            .bg(theme.sidebar)
            .border_b_1()
            .border_color(theme.border)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(group.label().to_uppercase())
            .child(count.to_string())
            .into_any_element()
    }

    fn row_element(&self, row_ix: usize, cx: &Context<Self>) -> AnyElement {
        let row = &self.rows[row_ix];
        let theme = cx.theme();
        let selected = self.selected == Some(row_ix);
        let (icon, icon_color) = state_icon(row, theme);

        let mut sub = row.repo.clone();
        if let Some(number) = row.number {
            sub.push_str(&format!(" #{number}"));
        }
        if let Some(author) = &row.author {
            sub.push_str(&format!(" \u{b7} {author}"));
        }
        let check = row.checks.map(|state| match state {
            CheckState::Success => (IconName::Check, theme.success),
            CheckState::Failure => (IconName::X, theme.danger),
            CheckState::Pending => (IconName::Clock, theme.warning),
        });

        div()
            .id(("queue-row", row_ix))
            .test_support()
            .relative()
            .flex()
            .w_full()
            .h(px(ROW_HEIGHT))
            .px(px(10.))
            .py(px(5.))
            .gap(px(8.))
            .items_start()
            .border_b_1()
            .border_color(theme.border)
            .when(selected, |d| {
                d.bg(theme.primary.opacity(0.12)).child(
                    div().absolute().left_0().top_0().bottom_0().w(px(2.)).bg(theme.primary),
                )
            })
            .when(!selected, |d| d.hover(|s| s.bg(theme.accent.opacity(0.5))))
            .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                this.select(Some(row_ix), cx);
                if ev.click_count() >= 2 {
                    this.open(window, cx);
                }
            }))
            .child(div().mt(px(3.)).child(Icon::new(icon).with_size(px(14.)).text_color(icon_color)))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_sm()
                            .when(row.unread, |d| d.font_weight(FontWeight::SEMIBOLD))
                            .child(row.title.clone()),
                    )
                    .child(
                        h_flex()
                            .gap(px(4.))
                            .items_center()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(div().truncate().min_w_0().child(sub))
                            .when_some(check, |d, (icon, color)| {
                                d.child(Icon::new(icon).with_size(px(12.)).text_color(color))
                            }),
                    ),
            )
            .child(
                h_flex()
                    .flex_none()
                    .gap(px(6.))
                    .pt(px(2.))
                    .items_center()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .when(row.unread, |d| d.child(div().size(px(6.)).rounded_full().bg(theme.primary)))
                    .child(age(fl_sync::now(), row.updated_at)),
            )
            .into_any_element()
    }

    fn filter_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let label = match self.query.repo_id {
            None => "All repos".to_owned(),
            Some(id) => self
                .repos
                .iter()
                .find(|(_, repo_id)| *repo_id == id)
                .map_or_else(|| format!("Repo {id}"), |(name, _)| name.clone()),
        };
        let repos = self.repos.clone();
        let queue = cx.weak_entity();
        Button::new("queue-repo-filter").ghost().small().label(label).dropdown_caret(true).dropdown_menu(
            move |mut menu, _, _| {
                let entries = std::iter::once(("All repos".to_owned(), None))
                    .chain(repos.iter().map(|(name, id)| (name.clone(), Some(*id))));
                for (label, repo_id) in entries {
                    let queue = queue.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                        let _ = queue.update(cx, |q, cx| q.set_repo(repo_id, cx));
                    }));
                }
                menu
            },
        )
    }

    #[cfg(test)]
    pub(super) fn keys(&self) -> Vec<&str> {
        self.rows.iter().map(|r| r.key.as_str()).collect()
    }

    #[cfg(test)]
    pub(super) fn selected_key(&self) -> Option<&str> {
        self.selected.map(|ix| self.rows[ix].key.as_str())
    }
}

fn state_icon(row: &QueueRow, theme: &Theme) -> (IconName, Hsla) {
    match (row.is_pr, row.state) {
        (_, None) => (IconName::Bell, theme.muted_foreground),
        (true, Some(IssueState::Open)) => (IconName::GitPullRequest, theme.success),
        (true, Some(IssueState::Merged)) => (IconName::GitMerge, theme.magenta),
        (true, Some(IssueState::Closed)) => (IconName::GitPullRequestClosed, theme.danger),
        (false, Some(IssueState::Open)) => (IconName::CircleDot, theme.success),
        (false, Some(_)) => (IconName::CircleCheck, theme.magenta),
    }
}

impl Render for Queue {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.rows.is_empty() {
            bench::ready(
                match self.query.kind {
                    QueueKind::Inbox => "inbox",
                    QueueKind::Pulls => "pulls",
                    QueueKind::Issues => "issues",
                    QueueKind::Reviews => "reviews",
                },
                window,
                cx,
            );
        }
        let theme = cx.theme();
        let total: u32 = self.counts.iter().map(|(_, n)| n).sum();
        let header = h_flex()
            .flex_none()
            .h(px(36.))
            .px(px(12.))
            .gap(px(8.))
            .items_center()
            .border_b_1()
            .border_color(theme.border)
            .child(div().font_weight(FontWeight::SEMIBOLD).child(self.title.clone()))
            .child(
                div()
                    .px(px(6.))
                    .rounded_full()
                    .bg(theme.muted)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(total.to_string()),
            )
            .child(div().flex_1())
            .child(self.filter_button(cx));

        let body = if self.rows.is_empty() {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.muted_foreground)
                .child("Nothing here")
                .into_any_element()
        } else {
            let list = v_virtual_list(cx.entity(), "queue-list", self.sizes.clone(), |this, range, _, cx| {
                if range.end + LOAD_AHEAD >= this.items.len() {
                    this.load_more(cx);
                }
                let theme = cx.theme();
                range
                    .map(|ix| match this.items[ix] {
                        Item::Header(group) => this.header_element(group, theme),
                        Item::Row(row_ix) => this.row_element(row_ix, cx),
                    })
                    .collect()
            })
            .track_scroll(&self.scroll);
            div().relative().flex_1().min_h_0().child(list).vertical_scrollbar(&self.scroll).into_any_element()
        };

        v_flex()
            .key_context("Queue")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_prev))
            .on_action(cx.listener(Self::open_selected))
            .size_full()
            .child(header)
            .child(body)
    }
}

#[cfg(test)]
mod tests;
