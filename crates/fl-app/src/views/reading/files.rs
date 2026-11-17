//! Files tab of a pull request: the Review desk (mockup D). Changed files
//! with their Viewed state on the left, the whole diff in the centre
//! (gpui-component `Diff`, one virtualized list for all files), review threads
//! and checks in a right rail.
//!
//! Keys (bound in `views::init`, handled by the reading pane): `]` / `[` next
//! and previous file, `v` toggles Viewed on the current file. A viewed file is
//! collapsed to its header; Viewed is local (`pr_viewed_files`) and resets
//! when the head commit changes.

use std::collections::HashSet;
use std::rc::Rc;

use fl_db::queries::pr;
use fl_types::{IssueId, PageReq, PrCheck, ReviewThread};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::diff::{Diff, DiffDecoration, DiffFile, DiffFileStatus, DiffLinePosition, DiffLineRange, DiffMode, DiffSide, DiffState};
use gpui_kit::component::{ActiveTheme as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use super::tabs;
use crate::bench;
use crate::store::Store;

/// Review threads read per PR; the rail says when there are more.
const THREAD_LIMIT: u32 = 100;
const FILE_LIST_WIDTH: f32 = 220.;
const RAIL_WIDTH: f32 = 240.;

pub(super) enum Load {
    Loading,
    Failed(SharedString),
    Ready,
}

pub struct FilesTab {
    pr_id: IssueId,
    pub(super) load: Load,
    /// Parsed files in patch order; the file list and the diff share it.
    pub(super) files: Vec<DiffFile>,
    pub(super) viewed: Rc<HashSet<SharedString>>,
    threads: Vec<ReviewThread>,
    more_threads: bool,
    checks: Vec<PrCheck>,
    /// Index in `files` of the file `]`, `[` and `v` act on.
    pub(super) current: Option<usize>,
    pub(super) diff: Option<Entity<DiffState>>,
}

struct Loaded {
    files: Vec<DiffFile>,
    viewed: Vec<String>,
    threads: Vec<ReviewThread>,
    more_threads: bool,
}

impl FilesTab {
    pub fn new(pr_id: IssueId, checks: Vec<PrCheck>, cx: &mut Context<Self>) -> Self {
        let this = Self {
            pr_id,
            load: Load::Loading,
            files: Vec::new(),
            viewed: Rc::default(),
            threads: Vec::new(),
            more_threads: false,
            checks,
            current: None,
            diff: None,
        };
        this.start_load(cx);
        this
    }

    fn start_load(&self, cx: &mut Context<Self>) {
        let pr_id = self.pr_id;
        let store = Store::global(cx).read(cx);
        let patch = store.diff_patch(cx, pr_id);
        let local = store.read(cx, move |c| {
            let threads = pr::threads(c, pr_id, PageReq { offset: 0, limit: THREAD_LIMIT })?;
            Ok((pr::viewed_paths(c, pr_id)?, threads))
        });
        cx.spawn(async move |this, cx| {
            let loaded = async {
                let (_, patch) = patch.await?;
                let parse = cx.background_spawn(async move { DiffFile::parse(&patch).map_err(|e| e.to_string()) });
                let files = parse.await?;
                let (viewed, threads) = local.await.map_err(|e| e.to_string())?;
                Ok::<_, String>(Loaded { files, viewed, more_threads: threads.next.is_some(), threads: threads.items })
            }
            .await;
            let _ = this.update(cx, |this, cx| {
                match loaded {
                    Ok(loaded) => this.install(loaded, cx),
                    Err(e) => this.load = Load::Failed(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn install(&mut self, loaded: Loaded, cx: &mut Context<Self>) {
        let viewed: HashSet<SharedString> = loaded.viewed.into_iter().map(SharedString::from).collect();
        let files = loaded.files;
        let collapsed: Vec<SharedString> = files.iter().map(|f| f.path().clone()).filter(|p| viewed.contains(p)).collect();
        let diff = cx.new(|cx| {
            let mut state = DiffState::new(files.clone(), cx);
            for path in &collapsed {
                state.set_file_collapsed(path, true, cx);
            }
            state
        });
        self.current = files.iter().position(|f| !viewed.contains(f.path())).or((!files.is_empty()).then_some(0));
        self.files = files;
        self.viewed = Rc::new(viewed);
        self.threads = loaded.threads;
        // Open threads first; then by path and line.
        self.threads.sort_by_key(|t| (t.is_resolved, t.path.clone(), t.line));
        self.more_threads = loaded.more_threads;
        self.diff = Some(diff);
        self.load = Load::Ready;
    }

    pub fn set_checks(&mut self, checks: Vec<PrCheck>, cx: &mut Context<Self>) {
        self.checks = checks;
        cx.notify();
    }

    /// Changed files once the patch is read.
    pub fn file_count(&self) -> Option<usize> {
        matches!(self.load, Load::Ready).then_some(self.files.len())
    }

    fn select(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(file) = self.files.get(ix) else { return };
        self.current = Some(ix);
        let path = file.path().clone();
        if let Some(diff) = &self.diff {
            diff.update(cx, |diff, cx| diff.scroll_to_file(&path, cx));
        }
        cx.notify();
    }

    /// `]` / `[`: the next or previous file, wrapping.
    pub fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let n = self.files.len();
        if n == 0 {
            return;
        }
        let ix = match (self.current, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(ix), true) => (ix + 1) % n,
            (Some(ix), false) => (ix + n - 1) % n,
        };
        self.select(ix, cx);
    }

    /// `v`: toggle Viewed on the current file. Marking a file viewed collapses
    /// it and moves on to the next file not viewed yet.
    pub fn toggle_current(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self.current else { return };
        let path = self.files[ix].path().clone();
        let viewed = !self.viewed.contains(&path);
        self.set_viewed(path, viewed, cx);
        if viewed {
            let n = self.files.len();
            if let Some(next) = (1..n).map(|d| (ix + d) % n).find(|i| !self.viewed.contains(self.files[*i].path())) {
                self.select(next, cx);
            }
        }
    }

    fn set_viewed(&mut self, path: SharedString, viewed: bool, cx: &mut Context<Self>) {
        let set = Rc::make_mut(&mut self.viewed);
        if viewed { set.insert(path.clone()) } else { set.remove(&path) };
        if let Some(diff) = &self.diff {
            diff.update(cx, |diff, cx| diff.set_file_collapsed(&path, viewed, cx));
        }
        cx.notify();
        let store = Store::global(cx).read(cx);
        // A bench run leaves its database as it found it.
        if store.read_only() {
            return;
        }
        let (db, pr_id) = (store.db().clone(), self.pr_id);
        let write = cx.background_spawn({
            let path = path.clone();
            async move { db.write(|c| pr::set_viewed(c, pr_id, &path, viewed)) }
        });
        cx.spawn(async move |this, cx| {
            if write.await.is_err() {
                // Show the state the database holds.
                let _ = this.update(cx, |this, cx| {
                    let set = Rc::make_mut(&mut this.viewed);
                    if viewed { set.remove(&path) } else { set.insert(path.clone()) };
                    if let Some(diff) = &this.diff {
                        diff.update(cx, |diff, cx| diff.set_file_collapsed(&path, !viewed, cx));
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn toggle_mode(&mut self, cx: &mut Context<Self>) {
        let Some(diff) = &self.diff else { return };
        diff.update(cx, |diff, cx| {
            let mode = if diff.mode() == DiffMode::Unified { DiffMode::Split } else { DiffMode::Unified };
            diff.set_mode(mode, cx);
        });
    }

    fn show_thread(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(thread) = self.threads.get(ix) else { return };
        let Some(line) = thread.line else { return };
        let side = match thread.side {
            fl_types::DiffSide::Left => DiffSide::Original,
            fl_types::DiffSide::Right => DiffSide::Modified,
        };
        let path = SharedString::from(thread.path.clone());
        let start = thread.start_line.unwrap_or(line).min(line) as usize;
        if let Some(file) = self.files.iter().position(|f| *f.path() == path) {
            self.current = Some(file);
        }
        if let Some(diff) = &self.diff {
            diff.update(cx, |diff, cx| {
                diff.scroll_to_line(DiffLinePosition::new(path.clone(), side, line as usize), cx);
                diff.set_selected_lines(Some(DiffLineRange::new(path, side, start, line as usize)), cx);
            });
        }
        cx.notify();
    }

    fn file_list(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let viewed = self.files.iter().filter(|f| self.viewed.contains(f.path())).count();
        let rows = self.files.iter().enumerate().map(|(ix, file)| {
            let is_viewed = self.viewed.contains(file.path());
            let (letter, color) = match file.status() {
                DiffFileStatus::Added => ("A", theme.success),
                DiffFileStatus::Deleted => ("D", theme.danger),
                DiffFileStatus::Renamed | DiffFileStatus::Copied => ("R", theme.info),
                _ => ("M", theme.warning),
            };
            let name = file.path().rsplit('/').next().unwrap_or(file.path()).to_owned();
            h_flex()
                .id(("files-row", ix))
                .h(px(26.))
                .px(px(8.))
                .gap(px(6.))
                .items_center()
                .rounded(px(4.))
                .text_xs()
                .font_family(theme.mono_font_family.clone())
                .when(self.current == Some(ix), |d| d.bg(theme.accent).text_color(theme.accent_foreground))
                .when(is_viewed, |d| d.text_color(theme.muted_foreground))
                .hover(|d| d.bg(theme.accent.opacity(0.5)))
                .child(div().w(px(12.)).flex_none().text_color(color).child(letter))
                .child(
                    div()
                        .w(px(12.))
                        .flex_none()
                        .when(is_viewed, |d| d.child(Icon::new(IconName::Check).with_size(px(12.)).text_color(theme.success))),
                )
                .child(div().flex_1().min_w_0().truncate().child(name))
                .child(div().flex_none().text_color(theme.success).child(format!("+{}", file.additions())))
                .on_click(cx.listener(move |this, _, _, cx| this.select(ix, cx)))
        });
        v_flex()
            .id("files-list")
            .w(px(FILE_LIST_WIDTH))
            .flex_none()
            .h_full()
            .p(px(6.))
            .gap(px(1.))
            .overflow_y_scroll()
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.sidebar)
            .child(
                div()
                    .px(px(4.))
                    .pb(px(4.))
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("Files \u{b7} {viewed} of {} viewed", self.files.len())),
            )
            .children(rows)
            .into_any_element()
    }

    fn toolbar(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let current = self.current.and_then(|ix| self.files.get(ix));
        let is_viewed = current.is_some_and(|f| self.viewed.contains(f.path()));
        let split = self.diff.as_ref().is_some_and(|d| d.read(cx).mode() == DiffMode::Split);
        h_flex()
            .flex_none()
            .h(px(32.))
            .px(px(10.))
            .gap(px(8.))
            .items_center()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_xs()
                    .font_family(theme.mono_font_family.clone())
                    .children(current.map(|f| f.path().clone())),
            )
            .child(div().flex_none().text_xs().text_color(theme.muted_foreground).child("[ ] file"))
            .child(
                Button::new("files-mode")
                    .xsmall()
                    .ghost()
                    .label(if split { "Split" } else { "Unified" })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_mode(cx))),
            )
            .child(
                Button::new("files-viewed")
                    .xsmall()
                    .outline()
                    .selected(is_viewed)
                    .when(is_viewed, |b| b.icon(IconName::Check))
                    .label("Viewed")
                    .child(div().text_xs().text_color(theme.muted_foreground).child("V"))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_current(cx))),
            )
            .into_any_element()
    }

    fn rail(&self, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let open = self.threads.iter().filter(|t| !t.is_resolved).count();
        let cards = self.threads.iter().enumerate().map(|(ix, t)| {
            let place = match t.line {
                Some(line) => format!("{}:{line}", t.path.rsplit('/').next().unwrap_or(&t.path)),
                None => format!("{} (outdated)", t.path.rsplit('/').next().unwrap_or(&t.path)),
            };
            let comments = t.comments.iter().take(3).map(|c| {
                let body: String = c.body_md.as_deref().unwrap_or("").chars().take(280).collect();
                div()
                    .child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(c.author.clone()))
                    .child(div().text_color(theme.foreground).child(body))
            });
            v_flex()
                .id(("files-thread", ix))
                .flex_none()
                .rounded(px(6.))
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .when(t.is_resolved, |d| d.opacity(0.6))
                .when(t.line.is_some(), |d| d.cursor_pointer().hover(|d| d.border_color(theme.primary)))
                .child(
                    h_flex()
                        .px(px(8.))
                        .py(px(4.))
                        .gap(px(6.))
                        .border_b_1()
                        .border_color(theme.border)
                        .bg(theme.secondary)
                        .child(div().flex_1().min_w_0().truncate().font_family(theme.mono_font_family.clone()).child(place))
                        .when(t.is_resolved, |d| d.child(div().text_color(theme.muted_foreground).child("resolved"))),
                )
                .child(v_flex().px(px(8.)).py(px(6.)).gap(px(6.)).children(comments).when(t.comments.len() > 3, |d| {
                    d.child(div().text_color(theme.muted_foreground).child(format!("{} more replies", t.comments.len() - 3)))
                }))
                .on_click(cx.listener(move |this, _, _, cx| this.show_thread(ix, cx)))
        });
        let checks = self.checks.iter().map(|c| {
            let color = match c.conclusion.as_deref() {
                Some("SUCCESS") => theme.success,
                Some("FAILURE" | "TIMED_OUT" | "CANCELLED") => theme.danger,
                None => theme.warning,
                Some(_) => theme.muted_foreground,
            };
            h_flex()
                .gap(px(6.))
                .items_center()
                .text_xs()
                .child(div().size(px(7.)).flex_none().rounded_full().bg(color))
                .child(div().flex_1().min_w_0().truncate().child(c.name.clone()))
        });
        v_flex()
            .id("files-rail")
            .w(px(RAIL_WIDTH))
            .flex_none()
            .h_full()
            .p(px(8.))
            .gap(px(8.))
            .overflow_y_scroll()
            .border_l_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap(px(6.))
                    .text_sm()
                    .child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child("Threads"))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(format!("{open} open"))),
            )
            .when(self.threads.is_empty(), |d| {
                d.child(div().text_xs().text_color(theme.muted_foreground).child("No review threads."))
            })
            .children(cards)
            .when(self.more_threads, |d| {
                d.child(div().text_xs().text_color(theme.muted_foreground).child("More threads on GitHub."))
            })
            .when(!self.checks.is_empty(), |d| {
                d.child(div().pt(px(4.)).text_xs().text_color(theme.muted_foreground).child("CHECKS")).children(checks)
            })
            .into_any_element()
    }

    /// Tint the lines that carry a review thread.
    fn decorations(&self, cx: &App) -> Vec<DiffDecoration> {
        let tint = cx.theme().info.opacity(0.12);
        self.threads
            .iter()
            .filter(|t| !t.is_resolved)
            .filter_map(|t| {
                let line = t.line? as usize;
                let side = match t.side {
                    fl_types::DiffSide::Left => DiffSide::Original,
                    fl_types::DiffSide::Right => DiffSide::Modified,
                };
                let start = t.start_line.map_or(line, |s| (s as usize).min(line));
                Some(DiffDecoration::new(DiffLineRange::new(t.path.clone(), side, start, line)).with_background(tint))
            })
            .collect()
    }
}

impl Render for FilesTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let diff = match (&self.load, &self.diff) {
            (Load::Loading, _) => return tabs::message(&["Loading the diff\u{2026}"], cx),
            (Load::Failed(e), _) => return tabs::message(&["The diff is not available.", e], cx),
            (Load::Ready, _) if self.files.is_empty() => return tabs::message(&["No changed files."], cx),
            (Load::Ready, None) => return tabs::message(&["No changed files."], cx),
            (Load::Ready, Some(diff)) => diff.clone(),
        };
        bench::diff_shown(&diff, window, cx);
        let viewed = self.viewed.clone();
        let this = cx.entity().downgrade();
        let body = Diff::new(&diff)
            .size_full()
            .decorations(self.decorations(cx))
            .render_header_suffix(move |file, _, cx| {
                let path = file.path().clone();
                let on = viewed.contains(&path);
                let this = this.clone();
                Button::new(SharedString::from(format!("files-viewed-{path}")))
                    .xsmall()
                    .ghost()
                    .selected(on)
                    .when(on, |b| b.icon(IconName::Check))
                    .label("Viewed")
                    .text_color(cx.theme().muted_foreground)
                    .on_click(move |_, _, cx| {
                        let _ = this.update(cx, |this, cx| this.set_viewed(path.clone(), !on, cx));
                    })
            });
        h_flex()
            .size_full()
            .child(self.file_list(cx))
            .child(v_flex().flex_1().min_w_0().h_full().child(self.toolbar(cx)).child(div().flex_1().min_h_0().child(body)))
            .child(self.rail(cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;
