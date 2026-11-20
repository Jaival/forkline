//! Search: free text plus operators over the local cache (`fl_search`),
//! with snippets. Enter in the bar runs the query in this tab; `Enter` in
//! the list opens the hit.

use std::ops::Range;
use std::sync::Arc;

use fl_types::{DataChanged, SearchHit};
use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable as _, FontWeight, HighlightStyle,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, StyledText, Styled as _, Subscription, Window,
    div, px,
};

use super::Built;
use super::home::{item_ref, state_icon};
use super::queue::FocusFilter;
use super::rows::{self, Fetch, HasRows, Rows};
use crate::theme::ActivePalette as _;
use crate::bench;
use crate::store::Store;
use crate::view_ref::ViewRef;
use crate::workspace::SetViewRef;
use crate::theme::TextScale as _;

const ROW_HEIGHT: f32 = 52.;
const PAGE: u32 = 50;
const HIT_START: char = '\u{E000}';
const HIT_END: char = '\u{E001}';

const HELP: &[(&str, &str)] = &[
    ("is:pr  is:issue", "pull requests or issues"),
    ("is:open  is:closed", "state"),
    ("is:unread", "updated since you last opened it"),
    ("author:name", "opened by"),
    ("assignee:name", "assigned to"),
    ("label:bug", "has the label (quote names with spaces)"),
    ("repo:owner/name", "in one repository"),
    ("\"two words\"", "a phrase"),
    ("-word  -is:closed", "exclude"),
];

/// The snippet without its hit marks, and the byte ranges of the hits.
fn split_snippet(snippet: &str) -> (String, Vec<Range<usize>>) {
    let mut text = String::with_capacity(snippet.len());
    let mut hits = Vec::new();
    let mut start = None;
    for c in snippet.chars() {
        match c {
            HIT_START => start = Some(text.len()),
            HIT_END => {
                if let Some(s) = start.take()
                    && s < text.len()
                {
                    hits.push(s..text.len());
                }
            }
            c => text.push(c),
        }
    }
    (text, hits)
}

fn fetch(q: String) -> Fetch<SearchHit> {
    Arc::new(move |c, req| fl_search::search(c, &q, req))
}

pub struct SearchView {
    q: String,
    rows: Rows<SearchHit>,
    input: Entity<InputState>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl SearchView {
    pub fn build(q: &str, window: &mut Window, cx: &mut App) -> Built {
        let view = cx.new(|cx| Self::new(q.trim().to_owned(), window, cx));
        let this = view.read(cx);
        // An empty search starts in the bar, so typing goes there.
        let focus = if this.q.is_empty() { this.input.focus_handle(cx) } else { this.focus.clone() };
        Built { view: view.into(), focus }
    }

    fn new(q: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search issues and pull requests. Try is:pr author:name label:bug repo:owner/name -is:closed")
                .default_value(q.clone())
        });
        let subscriptions = vec![
            cx.subscribe(&Store::global(cx), |this, _, ev: &DataChanged, cx| {
                if !this.q.is_empty() && ev.scopes.iter().any(|s| s == "issues" || s == "search") {
                    this.rows.reload(cx);
                }
            }),
            cx.subscribe_in(&input, window, |this, input, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    let q = input.read(cx).value().trim().to_owned();
                    this.run(q, window, cx);
                }
            }),
        ];
        let mut rows = Rows::new(fetch(q.clone()), PAGE, |h: &SearchHit| h.row.id);
        if !q.is_empty() {
            rows.reset(fetch(q.clone()), cx);
        }
        Self { q, rows, input, focus: cx.focus_handle(), _subscriptions: subscriptions }
    }

    /// Run `q` in this tab (the tab records it) and give the keys to the list.
    fn run(&mut self, q: String, window: &mut Window, cx: &mut Context<Self>) {
        if q != self.q {
            window.dispatch_action(Box::new(SetViewRef(ViewRef::Search { q: q.clone() })), cx);
            self.q = q;
            self.rows.items.clear();
            if !self.q.is_empty() {
                self.rows.reset(fetch(self.q.clone()), cx);
            }
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn focus_bar(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    /// Focus the bar with the query selected, so typing replaces it (the
    /// rail's Search button on an open Search tab).
    pub(crate) fn select_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }

    /// Esc in the bar: back to the list, the text stays.
    fn leave_bar(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
    }

    fn row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.palette();
        let hit = &self.rows.items[ix];
        let r = &hit.row;
        let (icon, color) = state_icon(r, &theme);
        let (text, hits) = split_snippet(&hit.snippet);
        let prefix = if hit.comment_id.is_some() && !text.is_empty() { "In a comment: " } else { "" };
        let strong = HighlightStyle { color: Some(theme.text), font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() };
        let shift = prefix.len();
        let highlights = hits.into_iter().map(|h| (h.start + shift..h.end + shift, strong)).collect::<Vec<_>>();
        let text = format!("{prefix}{text}");
        rows::row_shell("search-row", ix, ROW_HEIGHT, self, cx)
            .child(Icon::new(icon).with_size(px(14.)).text_color(color))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .text_body()
                            .child(div().flex_none().text_color(theme.text_muted).child(format!("{} #{}", r.repo, r.number)))
                            .child(div().truncate().child(r.title.clone())),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_small()
                            .text_color(theme.text_muted)
                            .child(StyledText::new(text).with_highlights(highlights)),
                    ),
            )
            .into_any_element()
    }

    fn status(&self) -> String {
        if !self.rows.loaded {
            return "Searching\u{2026}".into();
        }
        if self.rows.failed {
            return "Search failed.".into();
        }
        let more = if self.rows.has_more() { "+" } else { "" };
        let ms = self.rows.first_page_ms.map(|ms| format!(" \u{b7} {ms:.0} ms")).unwrap_or_default();
        match self.rows.items.len() {
            0 => "No results.".into(),
            1 => format!("1{more} result{ms}"),
            n => format!("{n}{more} results{ms}"),
        }
    }

    #[cfg(test)]
    pub(super) fn ids(&self) -> Vec<i64> {
        self.rows.items.iter().map(|h| h.row.id).collect()
    }

    #[cfg(test)]
    pub(super) fn input(&self) -> &Entity<InputState> {
        &self.input
    }
}

impl HasRows for SearchView {
    type Row = SearchHit;

    fn rows(&self) -> &Rows<SearchHit> {
        &self.rows
    }

    fn rows_mut(&mut self) -> &mut Rows<SearchHit> {
        &mut self.rows
    }

    fn open_ref(&self, hit: &SearchHit) -> Option<ViewRef> {
        Some(item_ref(&hit.row))
    }
}

impl Render for SearchView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.q.is_empty() && self.rows.loaded {
            bench::ready("search", window, cx);
        }
        let theme = *cx.palette();
        let bar = h_flex()
            .key_context("SearchBar")
            .on_action(cx.listener(Self::leave_bar))
            .flex_none()
            .h(px(40.))
            .px(px(12.))
            .gap(px(8.))
            .items_center()
            .border_b_1()
            .border_color(theme.border)
            .child(Icon::new(gpui_kit::assets::IconName::Search).with_size(px(14.)).text_color(theme.text_muted))
            .child(div().flex_1().min_w_0().child(Input::new(&self.input).appearance(false).bordered(false)));
        let body = if self.q.is_empty() {
            v_flex()
                .flex_1()
                .p(px(16.))
                .gap(px(6.))
                .text_body()
                .child(div().text_color(theme.text_muted).child("Type a query and press Enter. Operators:"))
                .children(HELP.iter().map(|(op, what)| {
                    h_flex()
                        .gap(px(12.))
                        .child(div().w(px(160.)).font_family(cx.theme().mono_font_family.clone()).child(*op))
                        .child(div().text_color(theme.text_muted).child(*what))
                }))
                .into_any_element()
        } else {
            let status = h_flex()
                .flex_none()
                .h(px(26.))
                .px(px(12.))
                .items_center()
                .text_small()
                .text_color(theme.text_muted)
                .border_b_1()
                .border_color(theme.border)
                .child(div().id("search-status").child(self.status()));
            let list = if self.rows.items.is_empty() { div().flex_1().into_any_element() } else { rows::list("search-list", cx, self, Self::row) };
            v_flex().flex_1().min_h_0().child(status).child(list).into_any_element()
        };
        let root = v_flex()
            .key_context("List")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::focus_bar))
            .size_full()
            .child(bar)
            .child(body);
        rows::list_keys(root, cx)
    }
}

#[cfg(test)]
mod tests;
