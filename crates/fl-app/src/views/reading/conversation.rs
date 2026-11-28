//! Conversation tab: the description, comments, reviews and timeline events
//! of one item, oldest first, with markdown bodies. A divider marks what is
//! new since the last view; the tab opens scrolled to it.

use std::rc::Rc;

use fl_types::Timestamp;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement as _, ListAlignment, ListOffset, ListState,
    ParentElement as _, SharedString, Styled as _, TestSupportExt as _, div, list, px,
};

use super::{Detail, Seen};
use crate::smooth_scroll::SmoothWheel;
use crate::theme::{ActivePalette as _, RADIUS_LG, TextScale as _};
use crate::views::queue::age;

enum Body {
    Markdown(SharedString),
    Html(SharedString),
}

impl Body {
    fn new(md: Option<&String>, html: Option<&String>) -> Option<Self> {
        let present = |s: Option<&String>| s.filter(|s| !s.trim().is_empty()).map(|s| SharedString::from(s.clone()));
        present(md).map(Self::Markdown).or_else(|| present(html).map(Self::Html))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tone {
    Success,
    Danger,
    Muted,
}

#[derive(Debug, PartialEq)]
enum Kind {
    Description,
    Comment,
    /// A review: what the reviewer did, e.g. `approved`.
    Review(&'static str, Tone),
    /// A timeline event: `kind` with spaces.
    Event(String),
    /// Comments on GitHub beyond those cached.
    More(u32),
    /// "New since your last view", with what is new.
    Divider(String),
}

struct Entry {
    /// Element id of the body, unique within the item.
    key: SharedString,
    kind: Kind,
    author: String,
    at: Option<Timestamp>,
    body: Option<Body>,
}

fn review_verb(state: &str) -> (&'static str, Tone) {
    match state {
        "APPROVED" => ("approved", Tone::Success),
        "CHANGES_REQUESTED" => ("requested changes", Tone::Danger),
        "DISMISSED" => ("dismissed review", Tone::Muted),
        "PENDING" => ("pending review", Tone::Muted),
        _ => ("reviewed", Tone::Muted),
    }
}

/// "3 comments, 1 review, 2 commits": the parts that are not zero.
fn new_text(comments: usize, reviews: usize, commits: usize) -> String {
    let part = |n: usize, one: &str| match n {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {one}s")),
    };
    [part(comments, "comment"), part(reviews, "review"), part(commits, "commit")]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
}

/// Description first, then comments, reviews and events by time (entries
/// without a time last), then the count of comments not cached. The divider
/// goes before the first new comment or review (events do not move it), or
/// last when only commits are new; there is none when nothing is new.
fn entries(detail: &Detail, seen: &Seen) -> Vec<Entry> {
    let row = detail.row();
    let (body, created_at) = match detail {
        Detail::Issue { detail, .. } => (Body::new(detail.body_md.as_ref(), detail.body_html.as_ref()), detail.created_at),
        Detail::Pr(pr) => (Body::new(pr.body_md.as_ref(), pr.body_html.as_ref()), pr.created_at),
    };
    let description =
        Entry { key: "desc".into(), kind: Kind::Description, author: row.author.clone(), at: Some(created_at), body };

    let comment = |id: i64, author: &String, at, md, html| Entry {
        key: format!("c{id}").into(),
        kind: Kind::Comment,
        author: author.clone(),
        at: Some(at),
        body: Body::new(md, html),
    };
    let mut rest = Vec::new();
    let loaded = match detail {
        Detail::Issue { thread, .. } => {
            for c in &thread.comments {
                rest.push(comment(c.id, &c.author, c.created_at, c.body_md.as_ref(), c.body_html.as_ref()));
            }
            for e in &thread.timeline {
                rest.push(Entry {
                    key: format!("e{}", e.id).into(),
                    kind: Kind::Event(e.kind.replace('_', " ")),
                    author: e.actor.clone(),
                    at: Some(e.created_at),
                    body: None,
                });
            }
            thread.comments.len()
        }
        Detail::Pr(pr) => {
            for c in &pr.comments {
                rest.push(comment(c.id, &c.author, c.created_at, c.body_md.as_ref(), c.body_html.as_ref()));
            }
            for r in &pr.reviews {
                let (verb, tone) = review_verb(&r.state);
                rest.push(Entry {
                    key: format!("r{}", r.id).into(),
                    kind: Kind::Review(verb, tone),
                    author: r.author.clone(),
                    at: r.submitted_at,
                    body: Body::new(r.body_md.as_ref(), r.body_html.as_ref()),
                });
            }
            pr.comments.len()
        }
    };
    rest.sort_by_key(|e| e.at.unwrap_or(Timestamp::MAX));

    let is_new = |e: &Entry| matches!(e.kind, Kind::Comment | Kind::Review(..)) && seen.is_new(Some(&e.author), e.at);
    let comments = rest.iter().filter(|e| e.kind == Kind::Comment && is_new(e)).count();
    let reviews = rest.iter().filter(|e| matches!(e.kind, Kind::Review(..)) && is_new(e)).count();
    let commits = detail.pr().map_or(0, |pr| {
        pr.commits.iter().filter(|c| seen.is_new(c.author.as_deref(), c.authored_at)).count()
    });
    if comments + reviews + commits > 0 {
        let at = rest.iter().position(is_new).unwrap_or(rest.len());
        let text = format!("New since your last view \u{b7} {}", new_text(comments, reviews, commits));
        rest.insert(at, Entry { key: "new".into(), kind: Kind::Divider(text), author: String::new(), at: None, body: None });
    }

    let mut out = Vec::with_capacity(rest.len() + 2);
    out.push(description);
    out.extend(rest);
    let more = (row.comments as usize).saturating_sub(loaded);
    if more > 0 {
        out.push(Entry { key: "more".into(), kind: Kind::More(more as u32), author: String::new(), at: None, body: None });
    }
    out
}

pub(super) struct Conversation {
    entries: Rc<Vec<Entry>>,
    state: ListState,
    wheel: SmoothWheel,
}

impl Conversation {
    pub(super) fn new() -> Self {
        Self {
            entries: Rc::default(),
            state: ListState::new(0, ListAlignment::Top, px(800.)),
            wheel: SmoothWheel::default(),
        }
    }

    /// Forget the item (the pane shows another one next).
    pub(super) fn clear(&mut self) {
        self.entries = Rc::default();
        self.state.reset(0);
        self.wheel.stop();
    }

    /// Index of the "New since your last view" entry.
    pub(super) fn divider(&self) -> Option<usize> {
        self.entries.iter().position(|e| matches!(e.kind, Kind::Divider(_)))
    }

    /// The first entry on show.
    #[cfg(test)]
    pub(super) fn top_entry(&self) -> usize {
        self.state.logical_scroll_top().item_ix
    }

    /// How far the list is scrolled down.
    #[cfg(test)]
    pub(super) fn scroll_top(&self) -> f32 {
        f32::from(-self.state.scroll_px_offset_for_scrollbar().y)
    }

    /// Show `detail`. Called again with fresh data for the same item (keep
    /// the scroll position), and after `clear` for a new item (start at the
    /// divider, else at the top).
    pub(super) fn set(&mut self, detail: &Detail, seen: &Seen, cx: &mut App) {
        let refresh = !self.entries.is_empty();
        self.entries = Rc::new(entries(detail, seen));
        let top = self.state.logical_scroll_top();
        self.state.reset(self.entries.len());
        if refresh {
            self.state.scroll_to(top);
        } else if let Some(ix) = self.divider() {
            self.state.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) });
        }
        crate::bench::conversation_shown(&self.state, cx);
    }

    pub(super) fn render(&self, _cx: &App) -> AnyElement {
        let entries = self.entries.clone();
        let list = list(self.state.clone(), move |ix, _, cx| {
            let p = *cx.palette();
            let entry = &entries[ix];
            let when = entry.at.map(|at| age(fl_sync::now(), at));
            let item = div()
                .px(px(24.))
                .pt(px(if ix == 0 { 20. } else { 14. }))
                .when(ix + 1 == entries.len(), |d| d.pb(px(20.)));
            // Lines without a card sit under the cards' text, past the avatar.
            let aside = |text: String| {
                div()
                    .pl(px(42.))
                    .text_small()
                    .text_color(p.text_muted)
                    .child(text)
                    .into_any_element()
            };
            let content = match &entry.kind {
                Kind::More(n) => aside(format!("{n} more comments on GitHub")),
                Kind::Divider(text) => {
                    let rule = || div().flex_1().h(px(1.)).bg(p.accent);
                    h_flex()
                        .id("new-since")
                        .test_support()
                        .gap(px(10.))
                        .text_small()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.accent_text)
                        .child(rule())
                        .child(text.clone())
                        .child(rule())
                        .into_any_element()
                }
                Kind::Event(kind) => aside(format!("{} {kind} {}", entry.author, when.unwrap_or_default())),
                kind => {
                    let verb = match kind {
                        Kind::Review(verb, tone) => Some((
                            *verb,
                            match tone {
                                Tone::Success => p.open.fg,
                                Tone::Danger => p.closed.fg,
                                Tone::Muted => p.text_muted,
                            },
                        )),
                        _ => None,
                    };
                    let body = match &entry.body {
                        Some(Body::Markdown(text)) => {
                            Some(TextView::markdown(entry.key.clone(), text.clone()).selectable(true).into_any_element())
                        }
                        Some(Body::Html(text)) => {
                            Some(TextView::html(entry.key.clone(), text.clone()).selectable(true).into_any_element())
                        }
                        None if *kind == Kind::Description => {
                            Some(div().text_color(p.text_muted).child("No description.").into_any_element())
                        }
                        None => None,
                    };
                    h_flex()
                        .items_start()
                        .gap(px(12.))
                        .child(crate::rail::avatar(&entry.author, 30., &p))
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap(px(6.))
                                .pt(px(10.))
                                .px(px(14.))
                                .pb(px(12.))
                                .border_1()
                                .border_color(p.border)
                                .rounded(RADIUS_LG)
                                .child(
                                    h_flex()
                                        .gap(px(6.))
                                        .text_secondary()
                                        .child(
                                            div()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(p.text)
                                                .child(entry.author.clone()),
                                        )
                                        .when_some(verb, |d, (verb, color)| d.child(div().text_color(color).child(verb)))
                                        .children(when.map(|w| div().text_color(p.text_muted).child(format!("\u{b7} {w}")))),
                                )
                                .when_some(body, |d, body| d.child(div().text_color(p.text_body).child(body))),
                        )
                        .into_any_element()
                }
            };
            item.child(content).into_any_element()
        })
        .size_full();
        div()
            .id("conversation")
            .test_support()
            .relative()
            .size_full()
            .child(self.wheel.element(&self.state))
            .child(list)
            .vertical_scrollbar(&self.state)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use fl_types::{IssueDetail, IssueRow, IssueState, IssueThread, PrComment, PrDetail, PrReview};

    use super::*;

    fn row(comments: u32) -> IssueRow {
        IssueRow {
            id: 1,
            repo_id: 1,
            repo: "o/a".into(),
            number: 1,
            is_pr: true,
            state: IssueState::Open,
            title: "t".into(),
            author: "me".into(),
            comments,
            updated_at: 0,
            unread: false,
        }
    }

    fn pr(comments: Vec<PrComment>, reviews: Vec<PrReview>, total: u32) -> Detail {
        Detail::Pr(PrDetail {
            row: row(total),
            body_html: None,
            body_md: Some("body".into()),
            labels: Vec::new(),
            created_at: 0,
            base_ref: "main".into(),
            head_ref: "topic".into(),
            base_oid: String::new(),
            head_oid: String::new(),
            is_draft: false,
            mergeable: None,
            merge_state: None,
            review_decision: None,
            check_state: None,
            additions: 0,
            deletions: 0,
            changed_files: 0,
            review_requests: Vec::new(),
            pending_fields: Vec::new(),
            comments,
            reviews,
            checks: Vec::new(),
            commits: Vec::new(),
        })
    }

    fn comment(id: i64, at: i64) -> PrComment {
        PrComment { id, author: "bob".into(), body_html: None, body_md: Some("hi".into()), created_at: at }
    }

    fn review(id: i64, state: &str, at: Option<i64>) -> PrReview {
        PrReview { id, author: "bob".into(), state: state.into(), body_html: None, body_md: None, submitted_at: at }
    }

    fn keys(detail: &Detail) -> Vec<String> {
        keys_seen(detail, &Seen::default())
    }

    fn keys_seen(detail: &Detail, seen: &Seen) -> Vec<String> {
        entries(detail, seen).iter().map(|e| e.key.to_string()).collect()
    }

    fn seen(at: i64) -> Seen {
        Seen { at: Some(at), viewer: Some("me".into()) }
    }

    fn divider_text(detail: &Detail, seen: &Seen) -> Option<String> {
        entries(detail, seen).into_iter().find_map(|e| match e.kind {
            Kind::Divider(text) => Some(text),
            _ => None,
        })
    }

    #[test]
    fn description_first_then_by_time_untimed_reviews_last() {
        let detail = pr(vec![comment(1, 30), comment(2, 10)], vec![review(7, "PENDING", None), review(8, "APPROVED", Some(20))], 2);
        assert_eq!(keys(&detail), ["desc", "c2", "r8", "c1", "r7"]);
        let approved = entries(&detail, &Seen::default()).into_iter().find(|e| e.key == "r8").unwrap();
        assert_eq!(approved.kind, Kind::Review("approved", Tone::Success));
        assert!(approved.body.is_none());
    }

    #[test]
    fn counts_comments_not_cached() {
        assert_eq!(keys(&pr(vec![comment(1, 1)], Vec::new(), 1)), ["desc", "c1"]);
        let all = entries(&pr(vec![comment(1, 1)], Vec::new(), 5), &Seen::default());
        assert_eq!(all.last().unwrap().kind, Kind::More(4));
    }

    #[test]
    fn an_issue_without_a_body_keeps_its_description_entry() {
        let detail = Detail::Issue {
            detail: IssueDetail { row: row(0), body_html: None, body_md: Some("  ".into()), labels: Vec::new(), created_at: 0 },
            thread: IssueThread {
                assignees: Vec::new(),
                comments: Vec::new(),
                timeline: Vec::new(),
                linked: Vec::new(),
                pending_fields: Vec::new(),
            },
        };
        let all = entries(&detail, &seen(0));
        assert_eq!(all.len(), 1);
        assert!(all[0].body.is_none());
    }

    #[test]
    fn the_divider_goes_before_the_first_new_comment_or_review() {
        let detail = pr(vec![comment(1, 10), comment(2, 30), comment(3, 50)], vec![review(8, "APPROVED", Some(40))], 3);
        // Never opened: nothing is new.
        assert_eq!(keys(&detail), ["desc", "c1", "c2", "r8", "c3"]);
        // Opened after everything: nothing is new.
        assert_eq!(keys_seen(&detail, &seen(60)), ["desc", "c1", "c2", "r8", "c3"]);
        // Opened between the comments.
        assert_eq!(keys_seen(&detail, &seen(20)), ["desc", "c1", "new", "c2", "r8", "c3"]);
        assert_eq!(divider_text(&detail, &seen(20)).unwrap(), "New since your last view \u{b7} 2 comments, 1 review");
    }

    #[test]
    fn untimed_reviews_own_comments_and_events_are_not_new() {
        let mut mine = comment(2, 30);
        mine.author = "me".into();
        let detail = pr(vec![comment(1, 10), mine], vec![review(7, "PENDING", None)], 2);
        assert_eq!(keys_seen(&detail, &seen(20)), ["desc", "c1", "c2", "r7"]);

        let event = |id, at| fl_types::TimelineEvent {
            id,
            kind: "labeled".into(),
            actor: "bob".into(),
            created_at: at,
            payload_json: None,
        };
        let issue = Detail::Issue {
            detail: IssueDetail { row: row(0), body_html: None, body_md: None, labels: Vec::new(), created_at: 0 },
            thread: IssueThread {
                assignees: Vec::new(),
                comments: Vec::new(),
                timeline: vec![event(5, 30)],
                linked: Vec::new(),
                pending_fields: Vec::new(),
            },
        };
        assert_eq!(keys_seen(&issue, &seen(20)), ["desc", "e5"]);
    }

    #[test]
    fn new_commits_alone_put_the_divider_last() {
        let mut detail = pr(vec![comment(1, 10)], Vec::new(), 1);
        let Detail::Pr(d) = &mut detail else { unreachable!() };
        let commit = |oid: &str, author: &str, at| fl_types::PrCommit {
            oid: oid.into(),
            headline: String::new(),
            author: Some(author.into()),
            authored_at: Some(at),
        };
        d.commits = vec![commit("a", "bob", 30), commit("b", "me", 40), commit("c", "bob", 50)];
        assert_eq!(keys_seen(&detail, &seen(20)), ["desc", "c1", "new"]);
        assert_eq!(divider_text(&detail, &seen(20)).unwrap(), "New since your last view \u{b7} 2 commits");
    }
}
