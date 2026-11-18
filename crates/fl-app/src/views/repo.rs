//! Repo overview: header with Pin, open PR / issue cards (they open the
//! repo's queues), languages, star history, recent commits, releases and
//! contributors. `p` pins.

use fl_db::queries::repo;
use fl_types::{DataChanged, LanguageShare, RepoOverview};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, FocusHandle, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, PathBuilder, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _,
    Subscription, TestSupportExt as _, Window, canvas, div, point, px, rgb,
};

use super::repos::set_pinned;
use super::{Built, TogglePin};
use crate::bench;
use crate::store::Store;
use crate::view_ref::ViewRef;
use crate::workspace::OpenView;

enum Load {
    Loading,
    Missing,
    Failed,
    Ready(Box<RepoOverview>),
}

pub struct RepoView {
    repo_id: i64,
    load: Load,
    /// Bumped per read; a stale read is dropped.
    generation: u32,
    scroll: ScrollHandle,
    focus: FocusHandle,
    _store: Subscription,
}

/// Percent share per language, largest first. Empty when there are no bytes.
fn language_shares(languages: &[LanguageShare]) -> Vec<(String, f32)> {
    let total: u64 = languages.iter().map(|l| l.bytes).sum();
    if total == 0 {
        return Vec::new();
    }
    let mut shares: Vec<_> =
        languages.iter().map(|l| (l.name.clone(), (l.bytes as f64 / total as f64 * 100.) as f32)).collect();
    shares.sort_by(|a, b| b.1.total_cmp(&a.1));
    shares
}

/// A stable colour per language name.
fn language_color(name: &str) -> u32 {
    const COLORS: [u32; 8] = [0xdea584, 0x3178c6, 0x00add8, 0x3572a5, 0x89e051, 0xe34c26, 0x563d7c, 0xb07219];
    let h = name.chars().fold(0u32, |h, c| h.wrapping_mul(31).wrapping_add(c as u32));
    COLORS[h as usize % COLORS.len()]
}

/// `YYYY-MM-DD` (UTC) of unix seconds.
fn date(secs: i64) -> String {
    // Howard Hinnant's civil_from_days.
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

impl RepoView {
    pub fn build(repo_id: i64, _: &mut Window, cx: &mut App) -> Built {
        let view = cx.new(|cx| {
            let _store = cx.subscribe(&Store::global(cx), |this: &mut Self, _, ev: &DataChanged, cx| {
                if ev.scopes.iter().any(|s| s == "repos" || s == "issues") {
                    this.read(cx);
                }
            });
            let mut this = Self {
                repo_id,
                load: Load::Loading,
                generation: 0,
                scroll: ScrollHandle::new(),
                focus: cx.focus_handle(),
                _store,
            };
            this.read(cx);
            this
        });
        let focus = view.read(cx).focus.clone();
        Built { view: view.into(), focus }
    }

    /// Read the overview in the background; the old one stays on screen.
    fn read(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let (generation, id) = (self.generation, self.repo_id);
        let task = Store::global(cx).read(cx).read(cx, move |c| repo::overview(c, id));
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.load = match result {
                    Ok(Some(o)) => Load::Ready(Box::new(o)),
                    Ok(None) => Load::Missing,
                    Err(_) => Load::Failed,
                };
                cx.notify();
            });
        })
        .detach();
    }

    fn toggle_pin(&mut self, _: &TogglePin, _: &mut Window, cx: &mut Context<Self>) {
        if let Load::Ready(o) = &self.load {
            set_pinned(o.id, !o.pinned, cx);
        }
    }

    #[cfg(test)]
    pub(super) fn overview(&self) -> Option<&RepoOverview> {
        match &self.load {
            Load::Ready(o) => Some(o),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn is_missing(&self) -> bool {
        matches!(self.load, Load::Missing)
    }
}

fn section(title: impl Into<SharedString>, body: impl IntoElement, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .flex_1()
        .min_w_0()
        .gap(px(8.))
        .p(px(14.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme.border)
        .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme.muted_foreground).child(title.into().to_uppercase()))
        .child(body)
        .into_any_element()
}

fn muted(text: &'static str, cx: &App) -> AnyElement {
    div().text_sm().text_color(cx.theme().muted_foreground).child(text).into_any_element()
}

fn card(id: &'static str, count: u32, label: &'static str, view: ViewRef, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .id(id)
        .test_support()
        .flex_1()
        .p(px(14.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        .hover(|s| s.bg(theme.accent.opacity(0.5)))
        .on_click(move |_, window, cx| window.dispatch_action(Box::new(OpenView(view.clone())), cx))
        .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child(count.to_string()))
        .child(div().text_sm().text_color(theme.muted_foreground).child(label))
        .into_any_element()
}

fn languages(o: &RepoOverview, cx: &App) -> AnyElement {
    let shares = language_shares(&o.languages);
    if shares.is_empty() {
        return muted("No language data.", cx);
    }
    let theme = cx.theme();
    v_flex()
        .gap(px(8.))
        .child(
            h_flex().h(px(8.)).w_full().rounded_full().overflow_hidden().children(
                shares.iter().map(|(name, pct)| div().h_full().w(gpui_kit::relative(pct / 100.)).bg(rgb(language_color(name)))),
            ),
        )
        .child(h_flex().flex_wrap().gap_x(px(16.)).gap_y(px(4.)).text_sm().children(shares.iter().map(|(name, pct)| {
            h_flex()
                .gap(px(6.))
                .items_center()
                .child(div().size(px(8.)).rounded_full().bg(rgb(language_color(name))))
                .child(name.clone())
                .child(div().text_color(theme.muted_foreground).child(format!("{pct:.1}%")))
        })))
        .into_any_element()
}

/// A line through the star counts, scaled into its box.
fn sparkline(values: Vec<u32>, color: Hsla) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let min = values.iter().copied().min().unwrap_or(0) as f32;
            let span = (values.iter().copied().max().unwrap_or(0) as f32 - min).max(1.);
            let last = (values.len() - 1) as f32;
            let mut path = PathBuilder::stroke(px(2.));
            for (i, v) in values.iter().enumerate() {
                let x = bounds.origin.x + bounds.size.width * (i as f32 / last);
                let y = bounds.origin.y + bounds.size.height * (1. - (*v as f32 - min) / span);
                if i == 0 { path.move_to(point(x, y)) } else { path.line_to(point(x, y)) }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        },
    )
    .h(px(40.))
    .w_full()
    .into_any_element()
}

fn commits(o: &RepoOverview, cx: &App) -> AnyElement {
    if o.commits.is_empty() {
        return muted("No cached commits. Pin this repository to sync them.", cx);
    }
    let theme = cx.theme();
    v_flex()
        .children(o.commits.iter().map(|c| {
            let dot = match c.check_state.as_deref() {
                Some("SUCCESS") => theme.success,
                Some("FAILURE" | "ERROR") => theme.danger,
                Some("PENDING") => theme.warning,
                _ => theme.border,
            };
            h_flex()
                .gap(px(10.))
                .py(px(5.))
                .items_center()
                .text_sm()
                .border_b_1()
                .border_color(theme.border)
                .child(div().flex_none().size(px(8.)).rounded_full().bg(dot))
                .child(div().flex_1().min_w_0().truncate().child(c.headline.clone()))
                .child(
                    h_flex()
                        .flex_none()
                        .gap(px(6.))
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(c.author.clone().unwrap_or_else(|| "unknown".into()))
                        .child(div().font_family(theme.mono_font_family.clone()).child(c.oid.chars().take(7).collect::<String>()))
                        .child(date(c.authored_at)),
                )
        }))
        .into_any_element()
}

fn releases(o: &RepoOverview, cx: &App) -> AnyElement {
    if o.releases.is_empty() {
        return muted("No releases.", cx);
    }
    let theme = cx.theme();
    v_flex()
        .gap(px(4.))
        .children(o.releases.iter().map(|r| {
            h_flex()
                .gap(px(8.))
                .text_sm()
                .child(div().flex_none().font_family(theme.mono_font_family.clone()).child(r.tag.clone()))
                .child(div().flex_1().min_w_0().truncate().text_color(theme.muted_foreground).child(r.name.clone().unwrap_or_default()))
                .child(div().flex_none().text_xs().text_color(theme.muted_foreground).child(r.published_at.map(date).unwrap_or_default()))
        }))
        .into_any_element()
}

fn contributors(o: &RepoOverview, cx: &App) -> AnyElement {
    if o.contributors.is_empty() {
        return muted("No contributor data.", cx);
    }
    let theme = cx.theme();
    v_flex()
        .gap(px(4.))
        .children(o.contributors.iter().map(|c| {
            h_flex()
                .justify_between()
                .text_sm()
                .child(c.login.clone())
                .child(div().text_color(theme.muted_foreground).child(c.contributions.to_string()))
        }))
        .into_any_element()
}

impl RepoView {
    fn overview_element(&self, o: &RepoOverview, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (id, pinned) = (o.id, o.pinned);
        let stats = [
            format!("{} {} stars", if o.starred { "\u{2605}" } else { "\u{2606}" }, o.stars),
            format!("{} forks", o.forks),
            format!("Default branch: {}", o.default_branch),
            format!("License: {}", o.license.as_deref().unwrap_or("none")),
        ];
        let header = v_flex()
            .gap(px(6.))
            .child(
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .child(Icon::new(IconName::BookMarked).with_size(px(18.)).text_color(theme.muted_foreground))
                    .child(
                        h_flex()
                            .flex_1()
                            .gap(px(6.))
                            .text_xl()
                            .child(div().text_color(theme.muted_foreground).child(format!("{} /", o.owner)))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(o.name.clone())),
                    )
                    .child(
                        Button::new("repo-pin")
                            .small()
                            .when(!pinned, |b| b.primary())
                            .icon(IconName::Pin)
                            .label(if pinned { "Unpin" } else { "Pin" })
                            .on_click(move |_, _, cx| set_pinned(id, !pinned, cx)),
                    ),
            )
            .child(div().text_color(theme.muted_foreground).child(o.description.clone().unwrap_or_else(|| "No description.".into())))
            .child(h_flex().flex_wrap().gap_x(px(16.)).text_sm().text_color(theme.muted_foreground).children(stats));
        let history: Vec<u32> = o.star_history.iter().map(|p| p.stars).collect();
        let history = if history.len() < 2 { muted("Not enough history yet.", cx) } else { sparkline(history, theme.primary) };
        v_flex()
            .w_full()
            .max_w(px(900.))
            .gap(px(16.))
            .p(px(24.))
            .child(header)
            .child(
                h_flex()
                    .gap(px(16.))
                    .child(card("repo-open-prs", o.open_prs, "Open pull requests", ViewRef::Pulls { repo_id: Some(id) }, cx))
                    .child(card("repo-open-issues", o.open_issues, "Open issues", ViewRef::Issues { repo_id: Some(id) }, cx)),
            )
            .child(section("Languages", languages(o, cx), cx))
            .child(section("Star history", history, cx))
            .child(section(format!("Recent commits on {}", o.default_branch), commits(o, cx), cx))
            .child(
                h_flex()
                    .items_start()
                    .gap(px(16.))
                    .child(section("Latest releases", releases(o, cx), cx))
                    .child(section("Top contributors", contributors(o, cx), cx)),
            )
            .into_any_element()
    }
}

impl Render for RepoView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match &self.load {
            Load::Loading => super::empty("Loading repository\u{2026}", cx),
            Load::Missing => super::empty("Repository not found in the local cache.", cx),
            Load::Failed => super::empty("Could not read the local repository cache.", cx),
            Load::Ready(o) => {
                bench::ready("repo", window, cx);
                let content = self.overview_element(o, cx);
                div()
                    .relative()
                    .size_full()
                    .child(
                        div()
                            .id("repo-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .child(h_flex().w_full().justify_center().child(content)),
                    )
                    .vertical_scrollbar(&self.scroll)
                    .into_any_element()
            }
        };
        v_flex()
            .key_context("Repo")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::toggle_pin))
            .size_full()
            .child(body)
    }
}

#[cfg(test)]
mod tests;
