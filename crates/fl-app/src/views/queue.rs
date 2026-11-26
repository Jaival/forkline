//! The work queue: one grouped (or flat), virtualized list behind Inbox, Pull requests,
//! Issues and Review requests.

use std::collections::HashSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use fl_db::queries::{queue, repo, sync, views};
use fl_types::{
    Action, CheckState, DataChanged, IssueState, PAGE_MAX, PageReq, QueueGroup, QueueKind, QueueQuery, QueueRow,
    RowReason, SavedView,
};
use gpui_kit::assets::IconName;
use gpui_kit::base::VirtualListScrollHandle;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use crate::motion::DropdownMenu as _;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex, v_virtual_list};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Entity, EventEmitter, FocusHandle, FontWeight, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Render, ScrollStrategy, SharedString,
    Size, StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _, WeakEntity, Window,
    actions, div, px, size,
};
use rusqlite::Connection;

use crate::bench;
use crate::drafts::Drafts;
use crate::store::Store;
use crate::theme::{ActivePalette as _, GROUP_HEADER, PANE_HEADER, Palette, RADIUS, ROW_HEIGHT, TextScale as _, scaled};
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
/// group. `flat`: rows only, no headers. A `collapsed` group keeps its header
/// and hides its rows.
fn flatten(rows: &[QueueRow], flat: bool, collapsed: &HashSet<QueueGroup>) -> Vec<Item> {
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
        if !collapsed.contains(&row.group) {
            items.push(Item::Row(ix));
        }
    }
    items
}

/// How long "Done all" waits before it sends, so Undo can take it back:
/// GitHub has no way to restore a done notification.
const UNDO_DONE: Duration = Duration::from_secs(6);

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
    /// A pinned repo has not finished its first deep sync.
    syncing: bool,
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
    let (synced, pinned) = sync::deep_progress(conn)?;
    Ok(Snapshot { rows, next, counts: queue::counts(conn, query, cond.as_ref())?, syncing: synced < pinned })
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
    /// The rem size `sizes` were measured at; the font size changes it.
    sizes_rem: Pixels,
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
    /// Groups whose rows are hidden (Home folds its last two).
    collapsed: HashSet<QueueGroup>,
    /// A pinned repo has not finished its first deep sync: an empty list
    /// says so.
    syncing: bool,
    /// Inbox "Done all": thread ids hidden now and sent when the timer ends,
    /// unless Undo drops the task first.
    pending_done: Vec<String>,
    _done_timer: Option<Task<()>>,
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
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter: is:open author:@me label:bug text"));
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
            sizes_rem: crate::theme::TEXT_BODY,
            scroll: VirtualListScrollHandle::new(),
            focus: cx.focus_handle(),
            repos: Rc::new(repos),
            filter: String::new(),
            input,
            filter_ms: None,
            filter_applied: false,
            views: saved,
            collapsed: if query.kind == QueueKind::Home {
                QueueGroup::ALL.into_iter().filter(|g| g.starts_collapsed()).collect()
            } else {
                HashSet::new()
            },
            syncing: false,
            pending_done: Vec::new(),
            _done_timer: None,
            _store: subscription,
            _input: input_subscription,
            _drafts: cx.observe(&Drafts::global(cx), |_, _, cx| cx.notify()),
        };
        if let Some(snap) = snap {
            this.rows = snap.rows;
            this.next = snap.next;
            this.counts = snap.counts;
            this.counts_ready = true;
            this.syncing = snap.syncing;
            this.rebuild();
        }
        this
    }

    fn rebuild(&mut self) {
        if !self.pending_done.is_empty() {
            let selected = self.selected.map(|ix| self.rows[ix].key.clone());
            self.rows.retain(|r| !self.pending_done.contains(&r.key));
            self.selected = selected.and_then(|key| self.rows.iter().position(|r| r.key == key));
        }
        self.items = flatten(&self.rows, self.query.flat, &self.collapsed);
        self.measure();
    }

    /// Click on a group header: fold or unfold its rows.
    fn toggle_group(&mut self, group: QueueGroup, cx: &mut Context<Self>) {
        if !self.collapsed.remove(&group) {
            self.collapsed.insert(group);
            if self.selected.is_some_and(|ix| self.rows[ix].group == group) {
                self.select(None, cx);
            }
        }
        self.rebuild();
        cx.notify();
    }

    /// Inbox: mark done every notification of `group` (all groups when
    /// `None`) under the current repo and filter; `closed_only` keeps those
    /// whose item is merged or closed. The rows hide at once; the actions go
    /// out after `UNDO_DONE` unless Undo comes first.
    pub(super) fn done_all(&mut self, group: Option<QueueGroup>, closed_only: bool, cx: &mut Context<Self>) {
        self.flush_done(cx);
        let query = self.query;
        let filter = self.filter.clone();
        let keys = Store::global(cx).read(cx).db().read(|c| {
            let cond = fl_search::queue_filter(c, &filter, query.kind)?;
            queue::inbox_keys(c, &query, cond.as_ref(), group, closed_only)
        });
        let Ok(keys) = keys else { return };
        if keys.is_empty() {
            return;
        }
        let had_selection = self.selected.is_some();
        self.pending_done = keys;
        self.rebuild();
        if had_selection && self.selected.is_none() {
            cx.emit(QueueEvent::Selected(None));
        }
        self._done_timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(UNDO_DONE).await;
            let _ = this.update(cx, |this, cx| this.flush_done(cx));
        }));
        cx.notify();
    }

    /// What an empty list says.
    pub(super) fn empty_text(&self) -> &'static str {
        if self.syncing {
            "Still syncing\u{2026}"
        } else if self.query.kind == QueueKind::Reviews && self.filter.trim().is_empty() {
            "No review requests. PRs on your repos are on Home."
        } else {
            "Nothing here"
        }
    }

    /// Send the pending "Done all" now.
    fn flush_done(&mut self, cx: &mut Context<Self>) {
        self._done_timer = None;
        let ids = std::mem::take(&mut self.pending_done);
        if ids.is_empty() {
            return;
        }
        let store = Store::global(cx).read(cx);
        if store.read_only() {
            return;
        }
        let engine = store.sync().clone();
        store
            .run(cx, async move {
                for id in ids {
                    engine.enqueue(Action::NotificationDone { id }).await?;
                }
                Ok::<_, fl_sync::Error>(())
            })
            .detach();
        cx.notify();
    }

    /// Undo a pending "Done all": nothing was sent; read the rows again.
    fn undo_done(&mut self, cx: &mut Context<Self>) {
        self._done_timer = None;
        self.pending_done.clear();
        self.reload(cx);
        cx.notify();
    }

    /// The rows' heights at `sizes_rem` (they hold text: see `theme::scaled`).
    fn measure(&mut self) {
        let scale = self.sizes_rem / crate::theme::TEXT_BODY;
        let sizes = self.items.iter().map(|item| {
            let height = if matches!(item, Item::Header(_)) { GROUP_HEADER } else { ROW_HEIGHT };
            size(px(0.), px(height * scale))
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

    /// j / k: the next shown row (rows of folded groups are skipped).
    fn move_selection(&mut self, step: isize, cx: &mut Context<Self>) {
        let shown: Vec<(usize, usize)> = self
            .items
            .iter()
            .enumerate()
            .filter_map(|(item_ix, item)| match item {
                Item::Row(ix) => Some((item_ix, *ix)),
                Item::Header(_) => None,
            })
            .collect();
        let Some(last) = shown.len().checked_sub(1) else { return };
        let at = self.selected.and_then(|sel| shown.iter().position(|(_, ix)| *ix == sel));
        let pos = match at {
            None => 0,
            Some(pos) => pos.saturating_add_signed(step).min(last),
        };
        let (item_ix, ix) = shown[pos];
        self.select(Some(ix), cx);
        self.scroll.scroll_to_item(item_ix, ScrollStrategy::Nearest);
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
            QueueKind::Inbox | QueueKind::Reviews | QueueKind::Home => return,
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
            self.syncing = snap.syncing;
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
        self.syncing = snap.syncing;
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

    fn header_element(&self, group: QueueGroup, cx: &Context<Self>) -> AnyElement {
        let p = cx.palette();
        let count = self.counts.iter().find(|(g, _)| *g == group).map_or(0, |(_, n)| *n);
        let collapsed = self.collapsed.contains(&group);
        // Home folds its groups; the Inbox clears one at a time.
        let foldable = self.query.kind == QueueKind::Home;
        let inbox = self.query.kind == QueueKind::Inbox;
        let ix = QueueGroup::ALL.iter().position(|g| *g == group).unwrap_or_default();
        h_flex()
            .id(("queue-group", ix))
            .test_support()
            .w_full()
            .h(scaled(GROUP_HEADER))
            .px(px(20.))
            .pt(px(10.))
            .pb(px(4.))
            .gap(px(6.))
            .items_center()
            .text_label()
            .font_weight(FontWeight::MEDIUM)
            .text_color(p.text_muted)
            .when(foldable, |d| {
                d.cursor_pointer()
                    .hover(|s| s.text_color(p.text_strong))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_group(group, cx)))
                    .child(
                        Icon::new(if collapsed { IconName::ChevronRight } else { IconName::ChevronDown })
                            .with_size(px(12.))
                            .flex_none(),
                    )
            })
            .child(group.label())
            .when(self.counts_ready, |d| d.child(div().text_color(p.text_faint).child(count.to_string())))
            .when(inbox, |d| {
                d.child(div().flex_1()).child(
                    Button::new(("queue-group-done", ix))
                        .ghost()
                        .xsmall()
                        .label("Done all")
                        .on_click(cx.listener(move |this, _, _, cx| this.done_all(Some(group), false, cx))),
                )
            })
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
                .when(row.bot, |d| {
                    d.child(
                        div()
                            .flex_none()
                            .px(px(4.))
                            .rounded(px(3.))
                            .border_1()
                            .border_color(p.border_strong)
                            .text_label()
                            .child("bot"),
                    )
                })
        });
        let reason = row.reason.map(|reason| {
            let color = match reason {
                RowReason::ChecksFailed | RowReason::ChangesRequested => p.closed.fg,
                RowReason::Approved => p.open.fg,
                RowReason::NewComments(_) | RowReason::NewCommits => p.accent,
            };
            div().id(("queue-reason", row_ix)).test_support().text_color(color).child(reason.label())
        });

        div()
            .w_full()
            .h(scaled(ROW_HEIGHT))
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
                                    .text_body()
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
                                    .text_small()
                                    .text_color(p.text_muted)
                                    .overflow_hidden()
                                    .when(!row.repo.is_empty(), |d| d.child(crate::repo_color::dot(&row.repo, cx)))
                                    .child(div().flex_none().font_family(mono).text_label().child(repo))
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
                            .text_small()
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
                            .map(|d| match reason {
                                Some(reason) => d.child(reason),
                                None => d.child(div().size(px(7.)).rounded_full().when(row.unread, |d| d.bg(p.accent))),
                            }),
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
        button.dropdown_menu(repo_filter_menu(repos, queue))
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
            .text_secondary()
            .child(
                h_flex()
                    .key_context("QueryBar")
                    .on_action(cx.listener(Self::escape_filter))
                    .h(scaled(30.))
                    .pl(px(12.))
                    .pr(px(12.))
                    .gap(px(6.))
                    .items_center()
                    // A field, not text: its own fill and border set it apart
                    // from the ghost buttons beside it.
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .h(scaled(24.))
                            .px(px(8.))
                            .gap(px(6.))
                            .items_center()
                            .rounded(RADIUS)
                            .bg(p.control)
                            .border_1()
                            .border_color(p.border_strong)
                            .child(div().text_color(p.text_faint).child("/"))
                            .child(div().flex_1().min_w_0().child(Input::new(&self.input).appearance(false).bordered(false))),
                    )
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
                        .h(scaled(26.))
                        .px(px(12.))
                        .gap(px(8.))
                        .items_center()
                        .text_small()
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

    /// Keys of the rows on screen: folded groups hide theirs.
    #[cfg(test)]
    pub(super) fn shown_keys(&self) -> Vec<&str> {
        self.items
            .iter()
            .filter_map(|item| match item {
                Item::Row(ix) => Some(self.rows[*ix].key.as_str()),
                Item::Header(_) => None,
            })
            .collect()
    }

    #[cfg(test)]
    pub(super) fn rows_for_test(&self) -> &[QueueRow] {
        &self.rows
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
        if window.rem_size() != self.sizes_rem {
            self.sizes_rem = window.rem_size();
            self.measure();
        }
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
                    QueueKind::Home => "home",
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
            .h(scaled(PANE_HEADER))
            .pl(px(20.))
            .pr(px(12.))
            .gap(px(8.))
            .items_center()
            .child(
                div()
                    .text_pane_title()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(self.title.clone()),
            )
            .when(counts_ready, |d| {
                d.child(div().text_secondary().text_color(p.text_muted).child(total.to_string()))
            })
            .child(div().flex_1())
            .when(self.query.kind == QueueKind::Inbox, |d| {
                d.child(
                    Button::new("queue-done-closed")
                        .ghost()
                        .small()
                        .label("Done: merged / closed")
                        .tooltip("Mark done every notification whose issue or PR is merged or closed")
                        .on_click(cx.listener(|this, _, _, cx| this.done_all(None, true, cx))),
                )
            })
            .when(self.query.kind != QueueKind::Home, |d| d.child(self.filter_button(cx)));

        let undo = (!self.pending_done.is_empty()).then(|| {
            let n = self.pending_done.len();
            h_flex()
                .id("queue-undo")
                .test_support()
                .flex_none()
                .h(scaled(30.))
                .px(px(20.))
                .gap(px(8.))
                .items_center()
                .bg(p.raised)
                .text_secondary()
                .child(format!("{n} marked done"))
                .child(div().flex_1())
                .child(
                    Button::new("queue-undo-button")
                        .ghost()
                        .small()
                        .label("Undo")
                        .on_click(cx.listener(|this, _, _, cx| this.undo_done(cx))),
                )
        });

        let body = if self.rows.is_empty() {
            div()
                .id("queue-empty")
                .test_support()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.text_muted)
                .child(self.empty_text())
                .into_any_element()
        } else {
            let list = v_virtual_list(cx.entity(), "queue-list", self.sizes.clone(), |this, range, _, cx| {
                if range.end + LOAD_AHEAD >= this.items.len() {
                    this.load_more(cx);
                }
                range
                    .map(|ix| match this.items[ix] {
                        Item::Header(group) => this.header_element(group, cx),
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
            .children(undo)
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

/// The queue's repo filter: "All repos", then each pinned repo with its dot.
/// Also built by `--open-menu` (see `menu_preview.rs`).
pub fn repo_filter_menu(
    repos: Rc<Vec<(String, i64)>>,
    queue: WeakEntity<Queue>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |mut menu, _, _| {
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
    }
}

#[cfg(test)]
mod tests;
