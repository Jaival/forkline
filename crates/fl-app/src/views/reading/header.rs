//! Header of the reading pane: repo and number, title, state and meta line.

use fl_types::{IssueState, Label, PrDetail, QueueRow, User};
use gpui_kit::component::{ActiveTheme as _, Theme, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, FontWeight, Hsla, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, Styled as _, TestSupportExt as _, div, px, rgb,
};

use super::Detail;

/// What the header shows beside the detail: assignees and pending markers.
pub(super) struct Extra<'a> {
    pub assignees: &'a [User],
    pub labels_pending: bool,
    pub assignees_pending: bool,
}

/// `row` is the selected queue row (absent in an item tab); `detail` arrives a
/// moment later. Render from `detail` when it is there, else from `row`.
pub(super) fn render(row: Option<&QueueRow>, detail: Option<&Detail>, extra: Extra<'_>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let block = v_flex().flex_none().p(px(12.)).gap(px(4.)).border_b_1().border_color(theme.border);
    let (repo, number, title, state, author) = match (detail, row) {
        (Some(detail), _) => {
            let item = detail.row();
            (&item.repo, Some(item.number), &item.title, Some(item.state), Some(&item.author))
        }
        (None, Some(row)) => (&row.repo, row.number, &row.title, row.state, row.author.as_ref()),
        (None, None) => return block.into_any_element(),
    };

    let mut path = repo.clone();
    if let Some(number) = number {
        path.push_str(&format!(" \u{b7} #{number}"));
    }
    let is_draft = detail.and_then(Detail::pr).is_some_and(|pr| pr.is_draft);
    let badge = state.map(|state| match state {
        IssueState::Open if is_draft => ("Draft", theme.muted_foreground),
        IssueState::Open => ("Open", theme.success),
        IssueState::Merged => ("Merged", theme.magenta),
        IssueState::Closed => ("Closed", theme.danger),
    });
    let labels: &[Label] = match detail {
        Some(Detail::Issue { detail, .. }) => &detail.labels,
        Some(Detail::Pr(pr)) => &pr.labels,
        None => &[],
    };

    let meta = h_flex()
        .flex_wrap()
        .items_center()
        .gap(px(6.))
        .text_xs()
        .text_color(theme.muted_foreground)
        .children(badge.map(|(text, color)| pill(text, color, 0.15)))
        .when_some(detail.and_then(Detail::pr), |d, pr| {
            d.child(format!("{} \u{2192} {}", pr.row.author, pr.base_ref))
                .child(
                    h_flex()
                        .gap(px(4.))
                        .child(div().text_color(theme.success).child(format!("+{}", pr.additions)))
                        .child(div().text_color(theme.danger).child(format!("\u{2212}{}", pr.deletions))),
                )
                .when(!pr.checks.is_empty(), |d| d.child(check_summary(pr, theme)))
        })
        .when_some(detail, |d, detail| match detail {
            Detail::Issue { detail, .. } => {
                d.child(detail.row.author.clone()).child(format!("{} comments", detail.row.comments))
            }
            Detail::Pr(_) => d,
        })
        .when(detail.is_none(), |d| d.children(author.cloned()))
        .children(labels.iter().map(|label| {
            let color = parse_color(&label.color).unwrap_or(theme.muted_foreground);
            pill(label.name.clone(), color, 0.18)
        }))
        .when(extra.labels_pending, |d| d.child(pending("labels-pending", "labels pending", theme)))
        .when(detail.is_some() && !extra.assignees.is_empty(), |d| {
            let logins: Vec<&str> = extra.assignees.iter().map(|u| u.login.as_str()).collect();
            d.child(format!("Assigned: {}", logins.join(", ")))
        })
        .when(extra.assignees_pending, |d| d.child(pending("assignees-pending", "assignees pending", theme)));

    block
        .child(div().text_xs().text_color(theme.muted_foreground).child(path))
        .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(title.clone()))
        .child(meta)
        .into_any_element()
}

/// Small rounded tag: text in `color` on `color` at `fill` opacity.
fn pill(text: impl Into<SharedString>, color: Hsla, fill: f32) -> Div {
    div().px(px(6.)).rounded_full().bg(color.opacity(fill)).text_color(color).child(text.into())
}

/// A change GitHub has not confirmed yet.
fn pending(id: &'static str, text: &'static str, theme: &Theme) -> impl gpui_kit::IntoElement {
    div().id(id).test_support().child(pill(text, theme.muted_foreground, 0.15))
}

/// A dot and `passed / total checks`.
fn check_summary(pr: &PrDetail, theme: &Theme) -> Div {
    let passed = |c: &&fl_types::PrCheck| matches!(c.conclusion.as_deref(), Some("SUCCESS" | "NEUTRAL" | "SKIPPED"));
    let failed = |c: &fl_types::PrCheck| matches!(c.conclusion.as_deref(), Some("FAILURE" | "TIMED_OUT" | "CANCELLED"));
    let passed = pr.checks.iter().filter(passed).count();
    let color = if pr.checks.iter().any(failed) {
        theme.danger
    } else if passed == pr.checks.len() {
        theme.success
    } else {
        theme.warning
    };
    h_flex()
        .gap(px(4.))
        .items_center()
        .child(div().size(px(6.)).rounded_full().bg(color))
        .child(format!("{passed} / {} checks", pr.checks.len()))
}

/// GitHub label colour: six hex digits, no `#`.
pub(super) fn parse_color(hex: &str) -> Option<Hsla> {
    u32::from_str_radix(hex, 16).ok().filter(|_| hex.len() == 6).map(|c| rgb(c).into())
}
