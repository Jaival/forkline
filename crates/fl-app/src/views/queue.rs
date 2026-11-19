//! The work queue: one grouped (or flat), virtualized list behind Inbox, Pull requests,
//! Issues and Review requests.

use std::rc::Rc;
use std::time::Instant;

use fl_db::queries::{queue, repo, views};
use fl_types::{
    CheckState, DataChanged, IssueState, PAGE_MAX, PageReq, QueueGroup, QueueKind, QueueQuery, QueueRow,
    SavedView,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::VirtualListScrollHandle;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex, v_virtual_list};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Entity, EventEmitter, FocusHandle, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, ScrollStrategy, SharedString,
    Size, StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window, actions,
    div, px, size,
};
use rusqlite::Connection;

use crate::bench;
use crate::drafts::Drafts;
use crate::store::Store;
use crate::theme::{
    ActivePalette as _, GROUP_HEADER, LINE_BODY, LINE_LABEL, PANE_HEADER, Palette, RADIUS, ROW_HEIGHT, TEXT_BODY,
    TEXT_LABEL, TEXT_PANE_TITLE, TEXT_SECONDARY, TEXT_SMALL,
};
use crate::view_ref::ViewRef;
use crate::workspace::{OpenView, SaveLayout, SetViewRef};

actions!(queue, [SelectNext, SelectPrev, OpenSelected, FocusFilter, SaveView, ToggleGrouped]);

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

/// One header before the first row of each group. Rows arrive ordered by
/// group. `flat`: rows only, no headers.
fn flatten(rows: &[QueueRow], flat: bool) -> Vec<Item> {
    if flat {
        return (0..rows.len()).map(Item::Row).collect();
    }
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

/// The first `pages` pages and the group counts, under the `/` filter text.
fn snapshot(conn: &Connection, query: &QueueQuery, filter: &str, pages: usize) -> rusqlite::Result<Snapshot> {
    let cond = fl_search::queue_filter(conn, filter, query.kind)?;
    let mut rows = Vec::new();
    let mut offset = 0;
    let mut next = None;
    for _ in 0..pages {
        let page = queue::page(conn, query, cond.as_ref(), PageReq { offset, limit: PAGE_MAX })?;
        rows.extend(page.items);
        next = page.next;
        match next {
            Some(n) => offset = n,
            None => break,
        }
    }
    Ok(Snapshot { rows, next, counts: queue::counts(conn, query, cond.as_ref())? })
}

pub struct Queue {
    query: QueueQuery,
    title: SharedString,
    rows: Vec<QueueRow>,
    counts: Vec<(QueueGroup, u32)>,
    /// False between a filter change and its counts: show no numbers then.
    counts_ready: bool,
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
    /// The `/` filter text, as typed.
    filter: String,
    input: Entity<InputState>,
    /// Milliseconds from the last filter change to its first page applied.
    filter_ms: Option<f64>,
    /// A first page for the current filter was applied; the next frame tells the bench.
    filter_applied: bool,
    /// Saved views of every queue kind.
    views: Vec<SavedView>,
    _store: Subscription,
    _input: Subscription,
    /// Rows mark PRs with unsent inline drafts.
    _drafts: Subscription,
}

impl EventEmitter<QueueEvent> for Queue {}

impl Queue {
    pub fn new(query: QueueQuery, title: SharedString, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Read synchronously: the first frame must have rows.
        let db = Store::global(cx).read(cx).db().clone();
        let snap = db.read(|c| snapshot(c, &query, "", 1)).ok();
        let repos = db
            .read(repo::pinned)
            .map(|pinned| pinned.into_iter().map(|r| (format!("{}/{}", r.owner, r.name), r.id)).collect())
            .unwrap_or_default();
        let saved = db.read(views::list).unwrap_or_default();
        let subscription = cx.subscribe(&Store::global(cx), |this, _, ev: &DataChanged, cx| {
            if ev.scopes.iter().any(|s| matches!(s.as_str(), "inbox" | "issues" | "pr")) {
                this.reload(cx);
            }
            if ev.scopes.iter().any(|s| s == "views") {
                this.views = Store::global(cx).read(cx).db().read(views::list).unwrap_or_default();
                cx.notify();
            }
        });
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter: is:open author:me label:bug text"));
        let input_subscription = cx.subscribe_in(&input, window, |this, input, ev: &InputEvent, window, cx| match ev {
            InputEvent::Change => {
                let text = input.read(cx).value().to_string();
                this.change_filter(text, cx);
            }
            InputEvent::PressEnter { .. } => window.focus(&this.focus, cx),
            _ => {}
        });
        let mut this = Self {
            query,
            title,
            rows: Vec::new(),
            counts: Vec::new(),
            counts_ready: false,
            next: None,
            loading: false,
            generation: 0,
            selected: None,
            items: Vec::new(),
            sizes: Rc::default(),
            scroll: VirtualListScrollHandle::new(),
            focus: cx.focus_handle(),
            repos: Rc::new(repos),
            filter: String::new(),
            input,
            filter_ms: None,
            filter_applied: false,
            views: saved,
            _store: subscription,
            _input: input_subscription,
            _drafts: cx.observe(&Drafts::global(cx), |_, _, cx| cx.notify()),
        };
        if let Some(snap) = snap {
            this.rows = snap.rows;
            this.next = snap.next;
            this.counts = snap.counts;
            this.counts_ready = true;
            this.rebuild();
        }
        this
    }

    fn rebuild(&mut self) {
        self.items = flatten(&self.rows, self.query.flat);
        let sizes = self.items.iter().map(|item| {
            let height = if matches!(item, Item::Header(_)) { GROUP_HEADER } else { ROW_HEIGHT };
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

    /// The repo filter menu: filter, and have the tab record it, so its title
    /// names the repo and the saved layout reopens it.
    pub(super) fn pick_repo(&mut self, repo_id: Option<i64>, window: &mut Window, cx: &mut Context<Self>) {
        self.set_repo(repo_id, cx);
        let view_ref = match self.query.kind {
            QueueKind::Pulls => ViewRef::Pulls { repo_id },
            QueueKind::Issues => ViewRef::Issues { repo_id },
            QueueKind::Inbox | QueueKind::Reviews => return,
        };
        window.dispatch_action(Box::new(SetViewRef(view_ref)), cx);
    }

    fn set_repo(&mut self, repo_id: Option<i64>, cx: &mut Context<Self>) {
        self.query.repo_id = repo_id;
        self.generation += 1;
        self.loading = false;
        self.selected = None;
        let query = self.query;
        let filter = self.filter.clone();
        let snap = Store::global(cx).read(cx).db().read(|c| snapshot(c, &query, &filter, 1)).ok();
        self.rows = Vec::new();
        self.counts = Vec::new();
        self.counts_ready = false;
        self.next = None;
        if let Some(snap) = snap {
            self.rows = snap.rows;
            self.next = snap.next;
            self.counts = snap.counts;
            self.counts_ready = true;
        }
        self.rebuild();
        cx.emit(QueueEvent::Selected(None));
        cx.notify();
    }

    pub(super) fn flat(&self) -> bool {
        self.query.flat
    }

    /// `g`, the Grouped button, the palette: show the groups or one list,
    /// newest first. Reads again like a repo change; the tab saves it.
    pub(super) fn toggle_grouped(&mut self, _: &ToggleGrouped, window: &mut Window, cx: &mut Context<Self>) {
        self.query.flat = !self.query.flat;
        let repo_id = self.query.repo_id;
        self.set_repo(repo_id, cx);
        window.dispatch_action(Box::new(SaveLayout), cx);
    }

    /// Show `text` in the query bar and filter by it.
    pub(super) fn set_filter_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.set_value(text.to_owned(), window, cx));
        self.change_filter(text.to_owned(), cx);
    }

    /// The Unread button: add or remove `is:unread` in the filter text.
    fn toggle_unread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = toggle_term(&self.filter, UNREAD);
        self.set_filter_text(&text, window, cx);
    }

    /// Filter by `filter`: read its first page in the background, then the
    /// group counts. Nothing is selected until the page is on screen.
    fn change_filter(&mut self, filter: String, cx: &mut Context<Self>) {
        self.filter = filter;
        self.generation += 1;
        self.loading = false;
        self.counts_ready = false;
        self.filter_ms = None;
        self.select(None, cx);
        let generation = self.generation;
        let started = Instant::now();
        let query = self.query;
        let text = self.filter.clone();
        let store = Store::global(cx).read(cx);
        let task = store.read(cx, move |c| {
            let cond = fl_search::queue_filter(c, &text, query.kind)?;
            queue::page(c, &query, cond.as_ref(), PageReq { offset: 0, limit: PAGE_MAX })
        });
        cx.spawn(async move |this, cx| {
            let page = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                if let Ok(page) = page {
                    this.rows = page.items;
                    this.next = page.next;
                    this.rebuild();
                    this.filter_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
                    this.filter_applied = true;
                    cx.notify();
                    this.read_counts(generation, cx);
                }
            });
        })
        .detach();
    }

    /// The group counts of the current filter, in a second background read.
    fn read_counts(&mut self, generation: u32, cx: &mut Context<Self>) {
        let query = self.query;
        let text = self.filter.clone();
        let task = Store::global(cx).read(cx).read(cx, move |c| {
            let cond = fl_search::queue_filter(c, &text, query.kind)?;
            queue::counts(c, &query, cond.as_ref())
        });
        cx.spawn(async move |this, cx| {
            let counts = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                if let Ok(counts) = counts {
                    this.counts = counts;
                    this.counts_ready = true;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Escape in the query bar: drop the filter, then give the keys back to the list.
    fn escape_filter(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        if !self.filter.is_empty() {
            self.set_filter_text("", window, cx);
        }
        window.focus(&self.focus, cx);
    }

    /// Save the filter as a view named after its text.
    pub(super) fn save_view(&mut self, _: &SaveView, _: &mut Window, cx: &mut Context<Self>) {
        let text = self.filter.trim().to_owned();
        if text.is_empty() {
            return;
        }
        let kind = self.query.kind;
        let db = Store::global(cx).read(cx).db().clone();
        let task = cx.background_spawn(async move { db.write(|c| views::save(c, kind, &text, &text, fl_sync::now())) });
        cx.spawn(async move |_, cx| {
            if task.await.is_ok() {
                cx.update(|cx| Store::emit_changed(&["views"], cx));
            }
        })
        .detach();
    }

    /// The saved views of this queue's kind.
    fn kind_views(&self) -> impl Iterator<Item = &SavedView> {
        self.views.iter().filter(|v| v.kind == self.query.kind)
    }

    fn load_more(&mut self, cx: &mut Context<Self>) {
        let Some(offset) = self.next else { return };
        if self.loading {
            return;
        }
        self.loading = true;
        let generation = self.generation;
        let query = self.query;
        let filter = self.filter.clone();
        let task = Store::global(cx).read(cx).read(cx, move |c| {
            let cond = fl_search::queue_filter(c, &filter, query.kind)?;
            queue::page(c, &query, cond.as_ref(), PageReq { offset, limit: PAGE_MAX })
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
        let filter = self.filter.clone();
        let pages = self.rows.len().div_ceil(PAGE_MAX as usize).max(1);
        let task = Store::global(cx).read(cx).read(cx, move |c| snapshot(c, &query, &filter, pages));
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
        self.counts_ready = true;
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

    fn header_element(&self, group: QueueGroup, p: &Palette) -> AnyElement {
        let count = self.counts.iter().find(|(g, _)| *g == group).map_or(0, |(_, n)| *n);
        h_flex()
            .w_full()
            .h(px(GROUP_HEADER))
            .px(px(20.))
            .pt(px(10.))
            .pb(px(4.))
            .gap(px(6.))
            .items_center()
            .text_size(TEXT_LABEL)
            .line_height(LINE_LABEL)
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.text_muted)
            .child(group.label())
            .when(self.counts_ready, |d| d.child(div().text_color(p.text_faint).child(count.to_string())))
            .into_any_element()
    }

    fn row_element(&self, row_ix: usize, cx: &Context<Self>) -> AnyElement {
        let row = &self.rows[row_ix];
        let p = cx.palette();
        let mono = cx.theme().mono_font_family.clone();
        let selected = self.selected == Some(row_ix);
        let (icon, icon_color) = state_icon(row, p);

        let mut repo = row.repo.clone();
        if let Some(number) = row.number {
            repo.push_str(&format!(" #{number}"));
        }
        let drafts = row.issue_id.filter(|_| row.is_pr).is_some_and(|id| Drafts::global(cx).read(cx).count(id) > 0);
        let check = row.checks.map(|state| match state {
            CheckState::Success => (IconName::Check, p.open.fg),
            CheckState::Failure => (IconName::X, p.closed.fg),
            CheckState::Pending => (IconName::Clock, p.pending.fg),
        });
        let author = row.author.as_ref().map(|login| {
            h_flex()
                .gap(px(6.))
                .items_center()
                .min_w_0()
                .child(crate::rail::avatar(login, 14., p))
                .child(div().truncate().child(login.clone()))
        });

        div()
            .w_full()
            .h(px(ROW_HEIGHT))
            .px(px(8.))
            .child(
                h_flex()
                    .id(("queue-row", row_ix))
                    .test_support()
                    .size_full()
                    .px(px(12.))
                    .gap(px(12.))
                    .items_center()
                    .rounded(RADIUS)
                    .when(selected, |d| d.bg(p.selected))
                    .when(!selected, |d| d.hover(|s| s.bg(p.hover)))
                    .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                        this.select(Some(row_ix), cx);
                        if ev.click_count() >= 2 {
                            this.open(window, cx);
                        }
                    }))
                    .child(Icon::new(icon).with_size(px(16.)).flex_none().text_color(icon_color))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(3.))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(TEXT_BODY)
                                    .line_height(LINE_BODY)
                                    .map(|d| {
                                        if row.unread {
                                            d.font_weight(FontWeight::MEDIUM).text_color(p.text_strong)
                                        } else {
                                            d.text_color(p.text_read)
                                        }
                                    })
                                    .child(row.title.clone()),
                            )
                            .child(
                                h_flex()
                                    .gap(px(6.))
                                    .items_center()
                                    .text_size(TEXT_SMALL)
                                    .line_height(LINE_LABEL)
                                    .text_color(p.text_muted)
                                    .overflow_hidden()
                                    .when(!row.repo.is_empty(), |d| d.child(crate::repo_color::dot(&row.repo, cx)))
                                    .child(div().flex_none().font_family(mono).text_size(TEXT_LABEL).child(repo))
                                    .when_some(author, |d, author| d.child("\u{b7}").child(author))
                                    .when_some(check, |d, (icon, color)| {
                                        d.child(Icon::new(icon).with_size(px(12.)).flex_none().text_color(color))
                                    }),
                            ),
                    )
                    .child(
                        v_flex()
                            .flex_none()
                            .gap(px(6.))
                            .items_end()
                            .text_size(TEXT_SMALL)
                            .line_height(LINE_LABEL)
                            .text_color(p.text_muted)
                            .child(
                                h_flex()
                                    .gap(px(6.))
                                    .items_center()
                                    .when(drafts, |d| {
                                        d.child(
                                            div()
                                                .id(("queue-drafts", row_ix))
                                                .test_support()
                                                .px(px(5.))
                                                .rounded(px(3.))
                                                .border_1()
                                                .border_dashed()
                                                .border_color(p.pending.fg)
                                                .text_color(p.pending.fg)
                                                .child("drafts"),
                                        )
                                    })
                                    .child(age(fl_sync::now(), row.updated_at)),
                            )
                            .child(div().size(px(7.)).rounded_full().when(row.unread, |d| d.bg(p.accent))),
                    ),
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
        let button = Button::new("queue-repo-filter").outline().small().dropdown_caret(true);
        let button = match self.query.repo_id {
            None => button.label(label),
            Some(_) => button.child(
                h_flex().gap(px(6.)).items_center().child(crate::repo_color::dot(&label, cx)).child(label),
            ),
        };
        button.dropdown_menu(move |mut menu, _, _| {
            let all = queue.clone();
            menu = menu.item(PopupMenuItem::new("All repos").on_click(move |_, window, cx| {
                let _ = all.update(cx, |q, cx| q.pick_repo(None, window, cx));
            }));
            for (name, id) in repos.iter() {
                let queue = queue.clone();
                let (name, repo_id) = (name.clone(), Some(*id));
                menu = menu.item(
                    PopupMenuItem::element(move |_, cx| {
                        h_flex().gap(px(8.)).items_center().child(crate::repo_color::dot(&name, cx)).child(name.clone())
                    })
                    .on_click(move |_, window, cx| {
                        let _ = queue.update(cx, |q, cx| q.pick_repo(repo_id, window, cx));
                    }),
                );
            }
            menu
        })
    }

    /// The `/` row under the header, and the result line when a filter is set.
    fn query_bar(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let p = *cx.palette();
        let text = self.filter.trim();
        let saved = self.kind_views().any(|v| v.query == text);
        let views: Vec<String> = self.kind_views().map(|v| v.query.clone()).collect();
        let queue = cx.weak_entity();
        let unread = has_term(&self.filter, UNREAD);
        let total: u32 = self.counts.iter().map(|(_, n)| n).sum();
        let result = match (self.counts_ready, self.filter_ms) {
            (true, Some(ms)) => format!("{total} results \u{b7} {ms:.0} ms"),
            (false, Some(ms)) => format!("{ms:.0} ms"),
            _ => "Searching".to_owned(),
        };
        v_flex()
            .flex_none()
            .border_b_1()
            .border_color(p.border)
            .text_size(TEXT_SECONDARY)
            .child(
                h_flex()
                    .key_context("QueryBar")
                    .on_action(cx.listener(Self::escape_filter))
                    .h(px(30.))
                    .pl(px(20.))
                    .pr(px(12.))
                    .gap(px(6.))
                    .items_center()
                    .child(div().text_color(p.text_faint).child("/"))
                    .child(div().flex_1().min_w_0().child(Input::new(&self.input).appearance(false).bordered(false)))
                    .child(
                        Button::new("queue-grouped")
                            .ghost()
                            .small()
                            .label("Grouped")
                            .selected(!self.query.flat)
                            .on_click(cx.listener(|this, _, window, cx| this.toggle_grouped(&ToggleGrouped, window, cx))),
                    )
                    .child(
                        Button::new("queue-unread")
                            .ghost()
                            .small()
                            .label("Unread")
                            .selected(unread)
                            .on_click(cx.listener(|this, _, window, cx| this.toggle_unread(window, cx))),
                    )
                    .child(Button::new("queue-views").ghost().small().label("Views").dropdown_caret(true).dropdown_menu(
                        move |mut menu, _, _| {
                            for query in &views {
                                let (queue, text) = (queue.clone(), query.clone());
                                menu = menu.item(PopupMenuItem::new(query.clone()).on_click(move |_, window, cx| {
                                    let _ = queue.update(cx, |q, cx| q.set_filter_text(&text, window, cx));
                                }));
                            }
                            if !views.is_empty() {
                                menu = menu.separator();
                            }
                            let queue = queue.clone();
                            menu.item(PopupMenuItem::new("Clear filter").on_click(move |_, window, cx| {
                                let _ = queue.update(cx, |q, cx| q.set_filter_text("", window, cx));
                            }))
                        },
                    )),
            )
            .when(!self.filter.is_empty(), |d| {
                d.child(
                    h_flex()
                        .h(px(26.))
                        .px(px(12.))
                        .gap(px(8.))
                        .items_center()
                        .text_size(TEXT_SMALL)
                        .text_color(p.text_muted)
                        .child(div().id("queue-result-count").child(result))
                        .child(div().flex_1())
                        .when(!text.is_empty() && !saved, |d| {
                            d.child(
                                Button::new("queue-save-view")
                                    .ghost()
                                    .small()
                                    .label("Save view")
                                    .on_click(cx.listener(|this, _, window, cx| this.save_view(&SaveView, window, cx))),
                            )
                        }),
                )
            })
    }

    #[cfg(test)]
    pub(super) fn input(&self) -> &Entity<InputState> {
        &self.input
    }

    #[cfg(test)]
    pub(super) fn repo_id(&self) -> Option<i64> {
        self.query.repo_id
    }

    #[cfg(test)]
    pub(super) fn filter(&self) -> &str {
        &self.filter
    }

    #[cfg(test)]
    pub(super) fn view_queries(&self) -> Vec<&str> {
        self.views.iter().map(|v| v.query.as_str()).collect()
    }

    #[cfg(test)]
    pub(super) fn headers(&self) -> usize {
        self.items.iter().filter(|item| matches!(item, Item::Header(_))).count()
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

fn state_icon(row: &QueueRow, p: &Palette) -> (IconName, Hsla) {
    match (row.is_pr, row.state) {
        (_, None) => (IconName::Bell, p.neutral.fg),
        (true, Some(IssueState::Open)) => (IconName::GitPullRequest, p.open.fg),
        (true, Some(IssueState::Merged)) => (IconName::GitMerge, p.merged.fg),
        (true, Some(IssueState::Closed)) => (IconName::GitPullRequestClosed, p.closed.fg),
        (false, Some(IssueState::Open)) => (IconName::CircleDot, p.open.fg),
        (false, Some(_)) => (IconName::CircleCheck, p.merged.fg),
    }
}

impl Render for Queue {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.filter_applied) {
            bench::ready("filter", window, cx);
        }
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
        let p = *cx.palette();
        let total: u32 = self.counts.iter().map(|(_, n)| n).sum();
        let counts_ready = self.counts_ready;
        let header = h_flex()
            .flex_none()
            .h(px(PANE_HEADER))
            .pl(px(20.))
            .pr(px(12.))
            .gap(px(8.))
            .items_center()
            .child(
                div()
                    .text_size(TEXT_PANE_TITLE)
                    .line_height(px(24.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title.clone()),
            )
            .when(counts_ready, |d| {
                d.child(div().text_size(TEXT_SECONDARY).text_color(p.text_muted).child(total.to_string()))
            })
            .child(div().flex_1())
            .child(self.filter_button(cx));

        let body = if self.rows.is_empty() {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.text_muted)
                .child("Nothing here")
                .into_any_element()
        } else {
            let list = v_virtual_list(cx.entity(), "queue-list", self.sizes.clone(), |this, range, _, cx| {
                if range.end + LOAD_AHEAD >= this.items.len() {
                    this.load_more(cx);
                }
                let p = *cx.palette();
                range
                    .map(|ix| match this.items[ix] {
                        Item::Header(group) => this.header_element(group, &p),
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
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::save_view))
            .on_action(cx.listener(Self::toggle_grouped))
            .size_full()
            .child(header)
            .child(self.query_bar(cx))
            .child(body)
    }
}

/// The filter term the Unread button sets.
const UNREAD: &str = "is:unread";

/// `term` is one of the words of `filter` (any case).
fn has_term(filter: &str, term: &str) -> bool {
    filter.split_whitespace().any(|w| w.eq_ignore_ascii_case(term))
}

/// `filter` with `term` removed when present, else appended.
fn toggle_term(filter: &str, term: &str) -> String {
    if has_term(filter, term) {
        let words: Vec<&str> = filter.split_whitespace().filter(|w| !w.eq_ignore_ascii_case(term)).collect();
        words.join(" ")
    } else if filter.trim().is_empty() {
        term.to_owned()
    } else {
        format!("{} {term}", filter.trim_end())
    }
}

#[cfg(test)]
mod tests;
