//! Header of the reading pane: repo and number, title, state and meta line.

use fl_types::{CheckState, IssueState, Label, PrDetail, QueueRow, User};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, FontWeight, Hsla, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, Styled as _, TestSupportExt as _, div, px, rgb,
};

use super::Detail;
use crate::theme::{ActivePalette as _, Palette, StateColor, TextScale as _, scaled};

/// What the header shows beside the detail: assignees and pending markers.
pub(super) struct Extra<'a> {
    pub assignees: &'a [User],
    pub labels_pending: bool,
    pub assignees_pending: bool,
}

/// `row` is the selected queue row (absent in an item tab); `detail` arrives a
/// moment later. Render from `detail` when it is there, else from `row`.
pub(super) fn render(row: Option<&QueueRow>, detail: Option<&Detail>, extra: Extra<'_>, cx: &App) -> AnyElement {
    let p = cx.palette();
    let mono = cx.theme().mono_font_family.clone();
    let block = v_flex().flex_none().pt(px(18.)).px(px(24.)).pb(px(14.)).gap(px(8.));
    let (repo, number, title, state, author) = match (detail, row) {
        (Some(detail), _) => {
            let item = detail.row();
            (&item.repo, Some(item.number), &item.title, Some(item.state), Some(&item.author))
        }
        (None, Some(row)) => (&row.repo, row.number, &row.title, row.state, row.author.as_ref()),
        (None, None) => return block.into_any_element(),
    };

    let pr = detail.and_then(Detail::pr);
    let is_pr = pr.is_some() || row.is_some_and(|r| r.is_pr);
    let is_draft = pr.is_some_and(|pr| pr.is_draft);
    let badge = state.map(|state| match (state, is_pr) {
        (IssueState::Open, _) if is_draft => ("Draft", IconName::GitPullRequestDraft, p.neutral),
        (IssueState::Open, true) => ("Open", IconName::GitPullRequest, p.open),
        (IssueState::Open, false) => ("Open", IconName::CircleDot, p.open),
        (IssueState::Merged, _) => ("Merged", IconName::GitMerge, p.merged),
        (IssueState::Closed, true) => ("Closed", IconName::GitPullRequestClosed, p.closed),
        (IssueState::Closed, false) => ("Closed", IconName::CircleCheck, p.merged),
    });
    let labels: &[Label] = match detail {
        Some(Detail::Issue { detail, .. }) => &detail.labels,
        Some(Detail::Pr(pr)) => &pr.labels,
        None => &[],
    };
    let person = |login: &str| {
        h_flex()
            .gap(px(6.))
            .items_center()
            .child(crate::rail::avatar(login, 18., p))
            .child(div().text_color(p.text).child(login.to_owned()))
    };

    let meta = h_flex()
        .flex_wrap()
        .items_center()
        .gap(px(10.))
        .text_secondary()
        .text_color(p.text_muted)
        .children(badge.map(|(text, icon, color)| state_pill(text, icon, color)))
        .when_some(pr, |d, pr| {
            d.child(
                h_flex()
                    .gap(px(6.))
                    .items_center()
                    .child(person(&pr.row.author))
                    .child("wants to merge into")
                    .child(
                        div()
                            .px(px(6.))
                            .rounded(px(4.))
                            .bg(p.raised)
                            .text_color(p.accent_text)
                            .font_family(mono.clone())
                            .text_label()
                            .child(pr.base_ref.clone()),
                    ),
            )
            .child(
                h_flex()
                    .gap(px(4.))
                    .child(div().text_color(p.added).child(format!("+{}", pr.additions)))
                    .child(div().text_color(p.removed).child(format!("\u{2212}{}", pr.deletions))),
            )
            .when(!pr.checks.is_empty(), |d| d.child(check_summary(pr, p)))
        })
        .when_some(detail, |d, detail| match detail {
            Detail::Issue { detail, .. } => d
                .child(h_flex().gap(px(6.)).child(person(&detail.row.author)).child("opened this issue"))
                .child(format!("{} comments", detail.row.comments)),
            Detail::Pr(_) => d,
        })
        .when(detail.is_none(), |d| d.children(author.map(|a| person(a))))
        .children(labels.iter().map(|label| {
            let (border, text) = p.label(parse_color(&label.color).unwrap_or(p.text_muted));
            div()
                .px(px(8.))
                .rounded_full()
                .border_1()
                .border_color(border)
                .text_color(text)
                .child(label.name.clone())
        }))
        .when(extra.labels_pending, |d| d.child(pending("labels-pending", "labels pending", p)))
        .when(detail.is_some() && !extra.assignees.is_empty(), |d| {
            let logins: Vec<&str> = extra.assignees.iter().map(|u| u.login.as_str()).collect();
            d.child(format!("Assigned: {}", logins.join(", ")))
        })
        .when(extra.assignees_pending, |d| d.child(pending("assignees-pending", "assignees pending", p)));

    let mut path = h_flex()
        .gap(px(6.))
        .text_secondary()
        .text_color(p.text_muted)
        .child(div().font_family(mono).text_small().child(repo.clone()));
    if let Some(number) = number {
        path = path.child(format!("#{number}"));
    }
    block
        .child(path)
        .child(
            div()
                .text_reading_title()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text_strong)
                .child(title.clone()),
        )
        .child(meta)
        .into_any_element()
}

/// The state with its icon, on the state's fill.
fn state_pill(text: &'static str, icon: IconName, color: StateColor) -> Div {
    h_flex()
        .h(scaled(22.))
        .pl(px(7.))
        .pr(px(9.))
        .gap(px(5.))
        .items_center()
        .rounded_full()
        .bg(color.fill)
        .text_color(color.on_fill)
        .font_weight(FontWeight::MEDIUM)
        .child(Icon::new(icon).with_size(px(12.)))
        .child(text)
}

/// A change GitHub has not confirmed yet.
fn pending(id: &'static str, text: &'static str, p: &Palette) -> impl gpui_kit::IntoElement {
    div()
        .id(id)
        .test_support()
        .px(px(8.))
        .rounded_full()
        .bg(p.neutral.fill)
        .text_color(p.neutral.on_fill)
        .child(SharedString::from(text))
}

/// An icon and `passed of total checks`. The icon follows GitHub's roll-up
/// state (as the merge row does); the check list decides only without one.
fn check_summary(pr: &PrDetail, p: &Palette) -> Div {
    let passed = |c: &&fl_types::PrCheck| matches!(c.conclusion.as_deref(), Some("SUCCESS" | "NEUTRAL" | "SKIPPED"));
    let failed = |c: &fl_types::PrCheck| matches!(c.conclusion.as_deref(), Some("FAILURE" | "TIMED_OUT" | "CANCELLED"));
    let passed = pr.checks.iter().filter(passed).count();
    let state = pr.check_state.as_deref().and_then(CheckState::parse).unwrap_or(if pr.checks.iter().any(failed) {
        CheckState::Failure
    } else if passed == pr.checks.len() {
        CheckState::Success
    } else {
        CheckState::Pending
    });
    let (icon, color) = match state {
        CheckState::Failure => (IconName::CircleX, p.closed.fg),
        CheckState::Success => (IconName::CircleCheck, p.open.fg),
        CheckState::Pending => (IconName::Clock, p.pending.fg),
    };
    h_flex()
        .gap(px(5.))
        .items_center()
        .child(Icon::new(icon).with_size(px(13.)).text_color(color))
        .child(format!("{passed} of {} checks", pr.checks.len()))
}

/// GitHub label colour: six hex digits, no `#`.
pub(super) fn parse_color(hex: &str) -> Option<Hsla> {
    u32::from_str_radix(hex, 16).ok().filter(|_| hex.len() == 6).map(|c| rgb(c).into())
}
