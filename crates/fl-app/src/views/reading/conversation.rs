//! Conversation tab: the description, comments, reviews and timeline events
//! of one item, oldest first, with markdown bodies.

use std::rc::Rc;

use fl_types::Timestamp;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, FontWeight, IntoElement as _, ListAlignment, ListState, ParentElement as _, SharedString,
    Styled as _, div, list, px,
};

use super::Detail;
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

/// Description first, then comments, reviews and events by time (entries
/// without a time last), then the count of comments not cached.
fn entries(detail: &Detail) -> Vec<Entry> {
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
}

impl Conversation {
    pub(super) fn new() -> Self {
        Self { entries: Rc::default(), state: ListState::new(0, ListAlignment::Top, px(800.)) }
    }

    /// Forget the item (the pane shows another one next).
    pub(super) fn clear(&mut self) {
        self.entries = Rc::default();
        self.state.reset(0);
    }

    /// Show `detail`. Called again with fresh data for the same item (keep
    /// the scroll position), and after `clear` for a new item (start at the top).
    pub(super) fn set(&mut self, detail: &Detail, _cx: &mut App) {
        let refresh = !self.entries.is_empty();
        self.entries = Rc::new(entries(detail));
        let top = self.state.logical_scroll_top();
        self.state.reset(self.entries.len());
        if refresh {
            self.state.scroll_to(top);
        }
    }

    pub(super) fn render(&self, _cx: &App) -> AnyElement {
        let entries = self.entries.clone();
        let list = list(self.state.clone(), move |ix, _, cx| {
            let theme = cx.theme();
            let entry = &entries[ix];
            let when = entry.at.map(|at| age(fl_sync::now(), at));
            let item = div().px(px(12.)).pt(px(if ix == 0 { 12. } else { 8. })).when(ix + 1 == entries.len(), |d| d.pb(px(12.)));
            let content = match &entry.kind {
                Kind::More(n) => div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("{n} more comments on GitHub"))
                    .into_any_element(),
                Kind::Event(kind) => div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("{} {kind} {}", entry.author, when.unwrap_or_default()))
                    .into_any_element(),
                kind => {
                    let verb = match kind {
                        Kind::Review(verb, tone) => Some((
                            *verb,
                            match tone {
                                Tone::Success => theme.success,
                                Tone::Danger => theme.danger,
                                Tone::Muted => theme.muted_foreground,
                            },
                        )),
                        _ => None,
                    };
                    let body = match &entry.body {
                        Some(Body::Markdown(text)) => {
                            Some(TextView::markdown(entry.key.clone(), text.clone()).selectable(true).text_sm().into_any_element())
                        }
                        Some(Body::Html(text)) => {
                            Some(TextView::html(entry.key.clone(), text.clone()).selectable(true).text_sm().into_any_element())
                        }
                        None if *kind == Kind::Description => Some(
                            div().text_color(theme.muted_foreground).child("No description.").into_any_element(),
                        ),
                        None => None,
                    };
                    v_flex()
                        .border_1()
                        .border_color(theme.border)
                        .rounded(px(6.))
                        .child(
                            h_flex()
                                .gap(px(6.))
                                .px(px(8.))
                                .py(px(4.))
                                .bg(theme.sidebar)
                                .rounded_t(px(6.))
                                .when(body.is_some(), |d| d.border_b_1().border_color(theme.border))
                                .text_xs()
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.foreground)
                                        .child(entry.author.clone()),
                                )
                                .when_some(verb, |d, (verb, color)| d.child(div().text_color(color).child(verb)))
                                .children(when.map(|w| div().text_color(theme.muted_foreground).child(w))),
                        )
                        .when_some(body, |d, body| d.child(div().p(px(8.)).text_sm().child(body)))
                        .into_any_element()
                }
            };
            item.child(content).into_any_element()
        })
        .size_full();
        div().relative().size_full().child(list).vertical_scrollbar(&self.state).into_any_element()
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
        entries(detail).iter().map(|e| e.key.to_string()).collect()
    }

    #[test]
    fn description_first_then_by_time_untimed_reviews_last() {
        let detail = pr(vec![comment(1, 30), comment(2, 10)], vec![review(7, "PENDING", None), review(8, "APPROVED", Some(20))], 2);
        assert_eq!(keys(&detail), ["desc", "c2", "r8", "c1", "r7"]);
        let approved = entries(&detail).into_iter().find(|e| e.key == "r8").unwrap();
        assert_eq!(approved.kind, Kind::Review("approved", Tone::Success));
        assert!(approved.body.is_none());
    }

    #[test]
    fn counts_comments_not_cached() {
        assert_eq!(keys(&pr(vec![comment(1, 1)], Vec::new(), 1)), ["desc", "c1"]);
        let all = entries(&pr(vec![comment(1, 1)], Vec::new(), 5));
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
        let all = entries(&detail);
        assert_eq!(all.len(), 1);
        assert!(all[0].body.is_none());
    }
}
