//! Files, Commits and Checks tabs of a pull request. Files lists the changed
//! files only; the diff is the Review desk (plan step 5).

use fl_types::{FileStatus, PrCheck, PrDetail};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, div, px, uniform_list,
};

use super::FilesState;
use crate::views::queue::age;

const ROW_HEIGHT: f32 = 28.;

fn row(cx: &App) -> Div {
    h_flex()
        .flex_none()
        .w_full()
        .h(px(ROW_HEIGHT))
        .px(px(12.))
        .gap(px(8.))
        .items_center()
        .text_sm()
        .border_b_1()
        .border_color(cx.theme().border)
}

fn summary(cx: &App) -> Div {
    row(cx).text_xs().text_color(cx.theme().muted_foreground)
}

fn message(lines: &[&str], cx: &App) -> AnyElement {
    v_flex()
        .size_full()
        .items_center()
        .justify_center()
        .gap(px(4.))
        .text_color(cx.theme().muted_foreground)
        .children(lines.iter().enumerate().map(|(i, line)| {
            let text = div().child(line.to_string());
            if i == 0 { text } else { text.text_xs() }
        }))
        .into_any_element()
}

fn plus_minus(additions: u32, deletions: u32, cx: &App) -> Div {
    h_flex()
        .flex_none()
        .gap(px(4.))
        .text_xs()
        .child(div().text_color(cx.theme().success).child(format!("+{additions}")))
        .child(div().text_color(cx.theme().danger).child(format!("\u{2212}{deletions}")))
}

pub(super) fn files(state: &FilesState, _pr: &PrDetail, cx: &App) -> AnyElement {
    let files = match state {
        FilesState::Idle | FilesState::Loading => return message(&["Loading changed files\u{2026}"], cx),
        FilesState::Failed(e) => return message(&["Changed files are not available.", e], cx),
        FilesState::Ready(files) if files.is_empty() => return message(&["No changed files."], cx),
        FilesState::Ready(files) => files.clone(),
    };
    let (additions, deletions) = files.iter().fold((0, 0), |(a, d), f| (a + f.additions, d + f.deletions));
    let head = summary(cx).child(format!("{} {} \u{b7}", files.len(), if files.len() == 1 { "file" } else { "files" })).child(plus_minus(additions, deletions, cx));
    let list = uniform_list("reading-files", files.len(), move |range, _, cx| {
        let theme = cx.theme();
        range
            .map(|ix| {
                let file = &files[ix];
                let (letter, color) = match file.status {
                    FileStatus::Added => ("A", theme.success),
                    FileStatus::Modified => ("M", theme.warning),
                    FileStatus::Deleted => ("D", theme.danger),
                    FileStatus::Renamed => ("R", theme.info),
                };
                let path = match &file.old_path {
                    Some(old) => format!("{old} \u{2192} {}", file.path),
                    None => file.path.clone(),
                };
                row(cx)
                    .child(div().w(px(14.)).flex_none().text_xs().text_color(color).child(letter))
                    .child(div().flex_1().min_w_0().truncate().font_family(theme.mono_font_family.clone()).text_xs().child(path))
                    .child(if file.binary {
                        div().text_xs().text_color(theme.muted_foreground).child("binary")
                    } else {
                        plus_minus(file.additions, file.deletions, cx)
                    })
            })
            .collect()
    })
    .flex_1();
    v_flex().size_full().child(head).child(list).into_any_element()
}

pub(super) fn commits(pr: &PrDetail, cx: &App) -> AnyElement {
    if pr.commits.is_empty() {
        return message(&["No commits cached."], cx);
    }
    let theme = cx.theme();
    let now = fl_sync::now();
    let rows = pr.commits.iter().map(|c| {
        row(cx)
            .child(
                div()
                    .flex_none()
                    .font_family(theme.mono_font_family.clone())
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(c.oid.chars().take(7).collect::<String>()),
            )
            .child(div().flex_1().min_w_0().truncate().child(c.headline.clone()))
            .children(c.author.clone().map(|a| div().flex_none().text_xs().text_color(theme.muted_foreground).child(a)))
            .children(c.authored_at.map(|at| div().flex_none().text_xs().text_color(theme.muted_foreground).child(age(now, at))))
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
    let theme = cx.theme();
    let passed = pr.checks.iter().filter(|c| matches!(classify(c), CheckClass::Passed | CheckClass::Neutral)).count();
    let rows = sorted(&pr.checks).into_iter().map(|c| {
        let (icon, color) = match classify(c) {
            CheckClass::Passed => (IconName::Check, theme.success),
            CheckClass::Failed => (IconName::X, theme.danger),
            CheckClass::Pending => (IconName::Clock, theme.warning),
            CheckClass::Neutral => (IconName::Minus, theme.muted_foreground),
        };
        let result = c.conclusion.as_deref().unwrap_or(&c.status).to_lowercase().replace('_', " ");
        row(cx)
            .child(Icon::new(icon).with_size(px(14.)).text_color(color))
            .child(div().flex_1().min_w_0().truncate().child(c.name.clone()))
            .child(div().flex_none().text_xs().text_color(theme.muted_foreground).child(result))
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
