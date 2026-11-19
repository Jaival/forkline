//! Commits and Checks tabs of a pull request. The Files tab is `files.rs`.

use fl_types::{PrCheck, PrDetail};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};

use crate::theme::{ActivePalette as _, LINE_LABEL, TEXT_BODY, TEXT_SMALL};
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, div, px,
};

use crate::views::queue::age;

const ROW_HEIGHT: f32 = 32.;

fn row(cx: &App) -> Div {
    h_flex()
        .flex_none()
        .w_full()
        .h(px(ROW_HEIGHT))
        .px(px(24.))
        .gap(px(10.))
        .items_center()
        .text_size(TEXT_BODY)
        .border_b_1()
        .border_color(cx.palette().border)
}

fn summary(cx: &App) -> Div {
    row(cx).text_size(TEXT_SMALL).line_height(LINE_LABEL).text_color(cx.palette().text_muted)
}

pub(super) fn message(lines: &[&str], cx: &App) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .text_color(cx.palette().text_muted)
        .children(lines.iter().enumerate().map(|(i, line)| {
            let text = div().child(line.to_string());
            if i == 0 { text } else { text.text_size(TEXT_SMALL) }
        }))
        .into_any_element()
}

pub(super) fn commits(pr: &PrDetail, cx: &App) -> AnyElement {
    if pr.commits.is_empty() {
        return message(&["No commits cached."], cx);
    }
    let p = cx.palette();
    let mono = cx.theme().mono_font_family.clone();
    let now = fl_sync::now();
    let rows = pr.commits.iter().map(|c| {
        row(cx)
            .child(
                div()
                    .flex_none()
                    .font_family(mono.clone())
                    .text_size(TEXT_SMALL)
                    .text_color(p.accent_text)
                    .child(c.oid.chars().take(7).collect::<String>()),
            )
            .child(div().flex_1().min_w_0().truncate().child(c.headline.clone()))
            .children(c.author.clone().map(|a| div().flex_none().text_size(TEXT_SMALL).text_color(p.text_muted).child(a)))
            .children(c.authored_at.map(|at| div().flex_none().text_size(TEXT_SMALL).text_color(p.text_muted).child(age(now, at))))
    });
    v_flex()
        .size_full()
        .child(summary(cx).child(format!("{} commits", pr.commits.len())))
        .child(div().id("reading-commits").flex_1().min_h_0().overflow_y_scroll().children(rows))
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum CheckClass {
    Failed,
    Pending,
    Passed,
    Neutral,
}

fn classify(check: &PrCheck) -> CheckClass {
    match check.conclusion.as_deref() {
        Some("SUCCESS") => CheckClass::Passed,
        Some("FAILURE" | "TIMED_OUT" | "CANCELLED") => CheckClass::Failed,
        None => CheckClass::Pending,
        Some(_) => CheckClass::Neutral,
    }
}

/// Failures first, then pending, then the rest; by name inside each class.
fn sorted(checks: &[PrCheck]) -> Vec<&PrCheck> {
    let mut out: Vec<&PrCheck> = checks.iter().collect();
    out.sort_by(|a, b| {
        let class = |c: &PrCheck| match classify(c) {
            CheckClass::Neutral => CheckClass::Passed,
            class => class,
        };
        class(a).cmp(&class(b)).then_with(|| a.name.cmp(&b.name))
    });
    out
}

pub(super) fn checks(pr: &PrDetail, cx: &App) -> AnyElement {
    if pr.checks.is_empty() {
        return message(&["No checks on the head commit."], cx);
    }
    let p = cx.palette();
    let passed = pr.checks.iter().filter(|c| matches!(classify(c), CheckClass::Passed | CheckClass::Neutral)).count();
    let rows = sorted(&pr.checks).into_iter().map(|c| {
        let (icon, color) = match classify(c) {
            CheckClass::Passed => (IconName::Check, p.open.fg),
            CheckClass::Failed => (IconName::X, p.closed.fg),
            CheckClass::Pending => (IconName::Clock, p.pending.fg),
            CheckClass::Neutral => (IconName::Minus, p.neutral.fg),
        };
        let result = c.conclusion.as_deref().unwrap_or(&c.status).to_lowercase().replace('_', " ");
        row(cx)
            .child(Icon::new(icon).with_size(px(14.)).text_color(color))
            .child(div().flex_1().min_w_0().truncate().child(c.name.clone()))
            .child(div().flex_none().text_size(TEXT_SMALL).text_color(p.text_muted).child(result))
    });
    v_flex()
        .size_full()
        .child(summary(cx).child(format!("{passed} / {} checks passed", pr.checks.len())))
        .child(div().id("reading-checks").flex_1().min_h_0().overflow_y_scroll().children(rows))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(name: &str, conclusion: Option<&str>) -> PrCheck {
        PrCheck { id: 0, name: name.into(), status: "COMPLETED".into(), conclusion: conclusion.map(Into::into), url: None }
    }

    #[test]
    fn failures_first_then_pending_then_by_name() {
        let checks = [
            check("b", Some("SUCCESS")),
            check("a", Some("SKIPPED")),
            check("z", None),
            check("y", Some("TIMED_OUT")),
            check("c", Some("FAILURE")),
        ];
        let names: Vec<_> = sorted(&checks).iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["c", "y", "z", "a", "b"]);
    }
}
