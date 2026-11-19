//! Paged, selectable rows behind the flat lists of step 7 (Repositories,
//! Starred, Explore, Search, Home).
//!
//! A view keeps a [`Rows`] and implements [`HasRows`]; [`list_keys`] wires
//! `j` / `k` / `Enter`, [`row_shell`] gives a row its selection look and
//! clicks, [`list`] renders the virtualized list and loads the next page on
//! scroll. Reads run on a background thread; a `generation` drops stale ones.

use std::sync::Arc;
use std::time::Instant;

use fl_types::{Page, PageReq};

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    ScrollStrategy, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, UniformListScrollHandle,
    Window, div, px, uniform_list,
};
use rusqlite::Connection;

use super::queue::{OpenSelected, SelectNext, SelectPrev};
use crate::store::Store;
use crate::theme::{ActivePalette as _, LINE_BODY, RADIUS, TEXT_BODY};
use crate::view_ref::ViewRef;
use crate::workspace::OpenView;

/// Start the next page when the rendered range ends this close to the end.
const LOAD_AHEAD: usize = 20;

/// Reads one page of rows.
pub type Fetch<T> = Arc<dyn Fn(&Connection, PageReq) -> rusqlite::Result<Page<T>> + Send + Sync>;

pub struct Rows<T> {
    pub items: Vec<T>,
    next: Option<u32>,
    /// Index into `items`. Nothing is selected until the first move.
    pub selected: Option<usize>,
    loading: bool,
    generation: u32,
    fetch: Fetch<T>,
    page_size: u32,
    /// Stable key of a row, to keep the selection across reloads.
    key: fn(&T) -> i64,
    pub scroll: UniformListScrollHandle,
    /// The first page of the current fetch is in.
    pub loaded: bool,
    /// The last read failed.
    pub failed: bool,
    /// Milliseconds from [`Rows::reset`] to its first page.
    pub first_page_ms: Option<f64>,
}

/// A view that shows a [`Rows`].
pub trait HasRows: Sized + 'static {
    type Row: Send + 'static;
    fn rows(&self) -> &Rows<Self::Row>;
    fn rows_mut(&mut self) -> &mut Rows<Self::Row>;
    /// The tab `Enter` and double-click open for a row.
    fn open_ref(&self, row: &Self::Row) -> Option<ViewRef>;
}

impl<T: Send + 'static> Rows<T> {
    pub fn new(fetch: Fetch<T>, page_size: u32, key: fn(&T) -> i64) -> Self {
        Self {
            items: Vec::new(),
            next: None,
            selected: None,
            loading: false,
            generation: 0,
            fetch,
            page_size,
            key,
            scroll: UniformListScrollHandle::new(),
            loaded: false,
            failed: false,
            first_page_ms: None,
        }
    }

    /// Read the first page on this thread, so the first frame has rows.
    pub fn load_now(&mut self, cx: &App) {
        let fetch = self.fetch.clone();
        let req = PageReq { offset: 0, limit: self.page_size };
        match Store::global(cx).read(cx).db().read(|c| fetch(c, req)) {
            Ok(page) => {
                self.items = page.items;
                self.next = page.next;
                self.failed = false;
            }
            Err(_) => self.failed = true,
        }
        self.loaded = true;
    }

    pub fn selected_row(&self) -> Option<&T> {
        self.selected.map(|ix| &self.items[ix])
    }

    /// Move the selection `step` rows; returns whether it changed.
    pub fn move_by(&mut self, step: isize) -> bool {
        let next = super::step(self.selected, self.items.len(), step);
        if next == self.selected {
            return false;
        }
        self.selected = next;
        if let Some(ix) = next {
            self.scroll.scroll_to_item(ix, ScrollStrategy::Nearest);
        }
        true
    }

    /// Show the rows of `fetch` instead: no selection, first page in the
    /// background. The old rows stay until it arrives.
    pub fn reset<V: HasRows<Row = T>>(&mut self, fetch: Fetch<T>, cx: &mut Context<V>) {
        let started = Instant::now();
        self.fetch = fetch;
        self.selected = None;
        self.loaded = false;
        self.first_page_ms = None;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.read(1, false, cx, move |rows| rows.first_page_ms = Some(started.elapsed().as_secs_f64() * 1000.0));
    }

    /// Read again as many pages as are loaded; keep the selection by key.
    pub fn reload<V: HasRows<Row = T>>(&mut self, cx: &mut Context<V>) {
        self.read(0, true, cx, |_| {});
    }

    /// Read `pages` pages from the start (0: as many as are loaded now) and
    /// replace the rows; `keep` keeps the selection by key, else none.
    fn read<V: HasRows<Row = T>>(
        &mut self,
        pages: usize,
        keep: bool,
        cx: &mut Context<V>,
        done: impl FnOnce(&mut Rows<T>) + 'static,
    ) {
        self.generation += 1;
        self.loading = true;
        let pages = if pages == 0 { self.items.len().div_ceil(self.page_size as usize).max(1) } else { pages };
        let (fetch, page_size, generation) = (self.fetch.clone(), self.page_size, self.generation);
        let task = Store::global(cx).read(cx).read(cx, move |c| {
            let mut items = Vec::new();
            let mut next = Some(0);
            for _ in 0..pages {
                let Some(offset) = next else { break };
                let page = fetch(c, PageReq { offset, limit: page_size })?;
                items.extend(page.items);
                next = page.next;
            }
            Ok((items, next))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |v, cx| {
                let rows = v.rows_mut();
                if rows.generation != generation {
                    return;
                }
                rows.loading = false;
                rows.loaded = true;
                match result {
                    Ok((items, next)) => {
                        let before = keep.then(|| rows.selected_row().map(rows.key)).flatten();
                        rows.selected = before.and_then(|k| items.iter().position(|r| (rows.key)(r) == k));
                        rows.items = items;
                        rows.next = next;
                        rows.failed = false;
                    }
                    Err(_) => rows.failed = true,
                }
                done(rows);
                cx.notify();
            });
        })
        .detach();
    }

    /// Append the next page, if there is one and no read is in flight.
    fn load_more<V: HasRows<Row = T>>(&mut self, cx: &mut Context<V>) {
        let Some(offset) = self.next else { return };
        if self.loading {
            return;
        }
        self.loading = true;
        let (fetch, limit, generation) = (self.fetch.clone(), self.page_size, self.generation);
        let task = Store::global(cx).read(cx).read(cx, move |c| fetch(c, PageReq { offset, limit }));
        cx.spawn(async move |this, cx| {
            let page = task.await;
            let _ = this.update(cx, |v, cx| {
                let rows = v.rows_mut();
                if rows.generation != generation {
                    return;
                }
                rows.loading = false;
                if let Ok(page) = page {
                    rows.items.extend(page.items);
                    rows.next = page.next;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// More rows exist past the loaded ones.
    pub fn has_more(&self) -> bool {
        self.next.is_some()
    }
}

fn open<V: HasRows>(v: &V, window: &mut Window, cx: &mut App) {
    if let Some(view_ref) = v.rows().selected_row().and_then(|row| v.open_ref(row)) {
        window.dispatch_action(Box::new(OpenView(view_ref)), cx);
    }
}

/// Attach the list keys (`j` / `k` / arrows / `Enter`) to the view root.
pub fn list_keys<V: HasRows>(root: Div, cx: &mut Context<V>) -> Div {
    root.on_action(cx.listener(|v: &mut V, _: &SelectNext, _, cx| {
        if v.rows_mut().move_by(1) {
            cx.notify();
        }
    }))
    .on_action(cx.listener(|v: &mut V, _: &SelectPrev, _, cx| {
        if v.rows_mut().move_by(-1) {
            cx.notify();
        }
    }))
    .on_action(cx.listener(|v: &mut V, _: &OpenSelected, window, cx| open(v, window, cx)))
}

/// One row: fixed height, selection tint and bar, hover, click selects and
/// double-click opens.
pub fn row_shell<V: HasRows>(id: &'static str, ix: usize, height: f32, v: &V, cx: &Context<V>) -> impl gpui_kit::ParentElement + IntoElement + use<V> {
    let p = cx.palette();
    let selected = v.rows().selected == Some(ix);
    div()
        .id(ElementId::NamedInteger(id.into(), ix as u64))
        .test_support()
        .relative()
        .flex()
        .w_full()
        .h(px(height))
        .px(px(12.))
        .gap(px(12.))
        .items_center()
        .rounded(RADIUS)
        .text_size(TEXT_BODY)
        .line_height(LINE_BODY)
        .when(selected, |d| d.bg(p.selected))
        .when(!selected, |d| d.hover(|s| s.bg(p.hover)))
        .on_click(cx.listener(move |v: &mut V, ev: &ClickEvent, window, cx| {
            v.rows_mut().selected = Some(ix);
            cx.notify();
            if ev.click_count() >= 2 {
                open(v, window, cx);
            }
        }))
}

/// The virtualized list with a scrollbar. `row` renders one row by index.
pub fn list<V: HasRows>(
    id: &'static str,
    cx: &mut Context<V>,
    v: &V,
    row: impl Fn(&mut V, usize, &mut Context<V>) -> AnyElement + 'static,
) -> AnyElement {
    let rows = v.rows();
    let count = rows.items.len();
    let scroll = rows.scroll.clone();
    let list = uniform_list(
        id,
        count,
        cx.processor(move |v: &mut V, range: std::ops::Range<usize>, _, cx| {
            if range.end + LOAD_AHEAD >= v.rows().items.len() {
                v.rows_mut().load_more(cx);
            }
            range.map(|ix| row(v, ix, cx)).collect::<Vec<_>>()
        }),
    )
    .size_full()
    // The rows' inset: rows are full width so their right-hand columns line up.
    .px(px(8.))
    .track_scroll(&scroll);
    div().relative().flex_1().min_h_0().child(list).vertical_scrollbar(&scroll).into_any_element()
}
