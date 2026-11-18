//! Files tab of a pull request: the Review desk (mockup D). Changed files
//! with their Viewed state on the left, the whole diff in the centre
//! (gpui-component `Diff`, one virtualized list for all files), review threads
//! and checks in a right rail.
//!
//! Keys (bound in `views::init`, handled by the reading pane): `]` / `[` next
//! and previous file, `v` toggles Viewed on the current file. A viewed file is
//! collapsed to its header; Viewed is local (`pr_viewed_files`) and resets
//! when the head commit changes.
//!
//! Rail cards reply to a thread (`ReplyThread`) and resolve / unresolve it
//! (`ResolveThread`) through the optimistic action queue. A `pr` change
//! reloads the threads and keeps the card order.
//!
//! Inline comments: click or drag line numbers to select lines; the add
//! button at the end of the selection (or toolbar Comment) opens a box under
//! the last line. The add button shows only while lines are selected: with
//! a handler set the `Diff` builds one per visible line every frame, which
//! halves the scroll rate. "Add to
//! review" keeps the comment as a draft in this tab; the `R` composer sends
//! the drafts with the review (`SubmitReview.comments`).

use std::collections::HashSet;
use std::rc::Rc;

use fl_db::queries::pr;
use fl_types::{Action, DataChanged, DraftReviewComment, IssueId, PageReq, PrCheck, ReviewThread};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::diff::{
    Diff, DiffAnnotation, DiffDecoration, DiffEvent, DiffFile, DiffFileStatus, DiffLinePosition, DiffLineRange,
    DiffMode, DiffSide, DiffState,
};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    TestSupportExt as _, Window, div, px,
};

use super::action_bar::CancelReply;
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
    /// The open thread reply: the thread id and its box.
    pub(super) reply: Option<(i64, Entity<TextareaState>)>,
    _reply_subscription: Option<Subscription>,
    /// Focus before Reply; it returns there when the box closes.
    return_focus: Option<FocusHandle>,
    /// Inline comments for the next review, in the order written.
    pub(super) drafts: Vec<DraftReviewComment>,
    /// The open inline comment box: its range and text.
    pub(super) composer: Option<(DiffLineRange, Entity<TextareaState>)>,
    _composer_subscription: Option<Subscription>,
    /// The last thread action error, shown in the rail.
    pub(super) notice: Option<SharedString>,
    _store: Subscription,
    /// Line selection changes of the diff (they show the add button).
    _diff_events: Option<Subscription>,
}

struct Loaded {
    files: Vec<DiffFile>,
    viewed: Vec<String>,
    threads: Vec<ReviewThread>,
    more_threads: bool,
}

impl FilesTab {
    pub fn new(pr_id: IssueId, checks: Vec<PrCheck>, cx: &mut Context<Self>) -> Self {
        let store = cx.subscribe(&Store::global(cx), |this: &mut Self, _, ev: &DataChanged, cx| {
            if matches!(this.load, Load::Ready) && ev.scopes.iter().any(|s| s == "pr") {
                this.reload_threads(cx);
            }
        });
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
            reply: None,
            _reply_subscription: None,
            return_focus: None,
            drafts: Vec::new(),
            composer: None,
            _composer_subscription: None,
            notice: None,
            _store: store,
            _diff_events: None,
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
        self._diff_events = Some(cx.subscribe(&diff, |_, _, ev: &DiffEvent, cx| {
            if matches!(ev, DiffEvent::SelectionChanged(_) | DiffEvent::SelectionEnded(_)) {
                cx.notify();
            }
        }));
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

    /// Read the threads again (after a reply or resolve, or a sync). Cards
    /// keep their place; new threads go last.
    fn reload_threads(&mut self, cx: &mut Context<Self>) {
        let pr_id = self.pr_id;
        let task = Store::global(cx)
            .read(cx)
            .read(cx, move |c| pr::threads(c, pr_id, PageReq { offset: 0, limit: THREAD_LIMIT }));
        cx.spawn(async move |this, cx| {
            let Ok(page) = task.await else { return };
            let _ = this.update(cx, |this, cx| {
                let old: Vec<i64> = this.threads.iter().map(|t| t.id).collect();
                let mut threads = page.items;
                threads.sort_by_key(|t| {
                    (old.iter().position(|id| *id == t.id).unwrap_or(usize::MAX), t.is_resolved, t.path.clone(), t.line)
                });
                this.threads = threads;
                this.more_threads = page.next.is_some();
                cx.notify();
            });
        })
        .detach();
    }

    /// Open the reply box of thread `id` and focus it.
    pub(super) fn open_reply(&mut self, id: i64, window: &mut Window, cx: &mut Context<Self>) {
        let state = match &self.reply {
            Some((open, state)) if *open == id => state.clone(),
            _ => {
                let state = cx.new(|cx| {
                    let mut state = TextareaState::new(window, cx).placeholder("Reply");
                    state.set_auto_grow(2, 8, cx);
                    state
                });
                let subscription = cx.subscribe_in(&state, window, |this, _, ev: &InputEvent, window, cx| match ev {
                    InputEvent::PressEnter { secondary: true, .. } => this.send_reply(window, cx),
                    InputEvent::Change => cx.notify(),
                    _ => {}
                });
                self.reply = Some((id, state.clone()));
                self._reply_subscription = Some(subscription);
                state
            }
        };
        if self.return_focus.is_none() {
            self.return_focus = window.focused(cx);
        }
        state.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    pub(super) fn send_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((thread_id, state)) = &self.reply else { return };
        let body_md = state.read(cx).value().trim().to_owned();
        if body_md.is_empty() {
            return;
        }
        let thread_id = *thread_id;
        self.close_reply(window, cx);
        self.enqueue(Action::ReplyThread { thread_id, body_md }, cx);
    }

    fn cancel_reply(&mut self, _: &CancelReply, window: &mut Window, cx: &mut Context<Self>) {
        self.close_reply(window, cx);
    }

    fn close_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reply = None;
        self._reply_subscription = None;
        if let Some(focus) = self.return_focus.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    /// Open the inline comment box for `range` and focus it. A box already
    /// open moves to the new range and keeps its text.
    pub(super) fn open_composer(&mut self, range: DiffLineRange, window: &mut Window, cx: &mut Context<Self>) {
        let state = match self.composer.take() {
            Some((_, state)) => state,
            None => {
                let state = cx.new(|cx| {
                    let mut state = TextareaState::new(window, cx).placeholder("Leave a comment");
                    state.set_auto_grow(2, 10, cx);
                    state
                });
                self._composer_subscription =
                    Some(cx.subscribe_in(&state, window, |this, _, ev: &InputEvent, window, cx| match ev {
                        InputEvent::PressEnter { secondary: true, .. } => this.add_draft(window, cx),
                        InputEvent::Change => cx.notify(),
                        _ => {}
                    }));
                state
            }
        };
        self.composer = Some((range, state.clone()));
        if self.return_focus.is_none() {
            self.return_focus = window.focused(cx);
        }
        state.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    /// Toolbar Comment: open the box for the selected lines.
    pub(super) fn comment_on_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(range) = self.diff.as_ref().and_then(|d| d.read(cx).selected_lines()) else { return };
        self.open_composer(range, window, cx);
    }

    /// Keep the open box as a draft for the review.
    pub(super) fn add_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((range, state)) = &self.composer else { return };
        let body_md = state.read(cx).value().trim().to_owned();
        if body_md.is_empty() {
            return;
        }
        self.drafts.push(draft_for(range, body_md));
        self.close_composer(window, cx);
    }

    fn close_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer = None;
        self._composer_subscription = None;
        if let Some(focus) = self.return_focus.take() {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub(super) fn remove_draft(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.drafts.len() {
            self.drafts.remove(ix);
            cx.notify();
        }
    }

    /// The drafts, handed to a review that is being sent.
    pub fn take_drafts(&mut self, cx: &mut Context<Self>) -> Vec<DraftReviewComment> {
        cx.notify();
        std::mem::take(&mut self.drafts)
    }

    /// Put back drafts of a review the queue rejected, before any new ones.
    pub fn restore_drafts(&mut self, mut drafts: Vec<DraftReviewComment>, cx: &mut Context<Self>) {
        drafts.append(&mut self.drafts);
        self.drafts = drafts;
        cx.notify();
    }

    /// Draft cards and the open box, under their lines.
    fn annotations(&self) -> Vec<DiffAnnotation> {
        let drafts = self.drafts.iter().enumerate().map(|(ix, d)| {
            let side = match d.side {
                fl_types::DiffSide::Left => DiffSide::Original,
                fl_types::DiffSide::Right => DiffSide::Modified,
            };
            DiffAnnotation::line(("draft", ix), DiffLinePosition::new(d.path.clone(), side, d.line as usize))
        });
        let composer = self.composer.as_ref().map(|(range, _)| {
            let (side, line) = end_of(range);
            DiffAnnotation::line("draft-composer", DiffLinePosition::new(range.path().clone(), side, line))
        });
        drafts.chain(composer).collect()
    }

    /// Resolve an open thread, unresolve a resolved one.
    pub(super) fn toggle_resolved(&mut self, id: i64, cx: &mut Context<Self>) {
        let Some(thread) = self.threads.iter().find(|t| t.id == id) else { return };
        let resolved = !thread.is_resolved;
        self.enqueue(Action::ResolveThread { thread_id: id, resolved }, cx);
    }

    /// Queue `action`; a rejected action shows its error in the rail.
    fn enqueue(&mut self, action: Action, cx: &mut Context<Self>) {
        self.notice = None;
        let store = Store::global(cx).read(cx);
        let engine = store.sync().clone();
        let task = store.run(cx, async move { engine.enqueue(action).await });
        cx.spawn(async move |this, cx| {
            let error = match task.await {
                Ok(Ok(_)) => return,
                Ok(Err(e)) => e.to_string(),
                Err(e) => e.to_string(),
            };
            let _ = this.update(cx, |this, cx| {
                this.notice = Some(error.into());
                cx.notify();
            });
        })
        .detach();
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
                Button::new("files-comment")
                    .xsmall()
                    .ghost()
                    .label("Comment")
                    .disabled(self.diff.as_ref().is_none_or(|d| d.read(cx).selected_lines().is_none()))
                    .on_click(cx.listener(|this, _, window, cx| this.comment_on_selection(window, cx))),
            )
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
            // A resolved thread shows its first comment only, as on GitHub.
            let shown = if t.is_resolved { 1 } else { t.comments.len() };
            let comments = t.comments.iter().take(shown).map(|c| {
                let body: String = c.body_md.as_deref().unwrap_or("").chars().take(280).collect();
                div()
                    .child(
                        h_flex()
                            .gap(px(4.))
                            .child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(c.author.clone()))
                            // Local rows have negative ids until GitHub confirms them.
                            .when(c.id < 0, |d| d.child(div().text_color(theme.muted_foreground).child("sending"))),
                    )
                    .child(div().text_color(theme.foreground).child(body))
            });
            let (id, resolved) = (t.id, t.is_resolved);
            let reply = self.reply.as_ref().filter(|(open, _)| *open == id).map(|(_, state)| state.clone());
            let actions = h_flex()
                .px(px(4.))
                .pb(px(4.))
                .gap(px(2.))
                .when(!resolved && reply.is_none(), |d| {
                    d.child(
                        Button::new(("thread-reply", ix))
                            .xsmall()
                            .ghost()
                            .label("Reply")
                            .on_click(cx.listener(move |this, _, window, cx| this.open_reply(id, window, cx))),
                    )
                })
                .child(
                    Button::new(("thread-resolve", ix))
                        .xsmall()
                        .ghost()
                        .label(if resolved { "Unresolve" } else { "Resolve" })
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_resolved(id, cx))),
                );
            v_flex()
                .flex_none()
                .rounded(px(6.))
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .child(
                    v_flex()
                        .id(("files-thread", ix))
                        .when(resolved, |d| d.opacity(0.6))
                        .when(t.line.is_some(), |d| d.cursor_pointer().hover(|d| d.bg(theme.accent.opacity(0.3))))
                        .child(
                            h_flex()
                                .px(px(8.))
                                .py(px(4.))
                                .gap(px(6.))
                                .border_b_1()
                                .border_color(theme.border)
                                .bg(theme.secondary)
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .font_family(theme.mono_font_family.clone())
                                        .child(place),
                                )
                                .when(t.pending, |d| d.child(div().text_color(theme.muted_foreground).child("syncing")))
                                .when(resolved, |d| d.child(div().text_color(theme.muted_foreground).child("resolved"))),
                        )
                        .child(v_flex().px(px(8.)).py(px(6.)).gap(px(6.)).children(comments).when(
                            shown < t.comments.len(),
                            |d| {
                                let n = t.comments.len() - shown;
                                d.child(div().text_color(theme.muted_foreground).child(format!("{n} more replies")))
                            },
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| this.show_thread(ix, cx))),
                )
                .when_some(reply, |d, state| {
                    let empty = state.read(cx).value().trim().is_empty();
                    d.child(
                        v_flex()
                            .key_context("Reply")
                            .on_action(cx.listener(Self::cancel_reply))
                            .px(px(6.))
                            .pb(px(6.))
                            .gap(px(4.))
                            .child(Textarea::new(&state))
                            .child(
                                h_flex()
                                    .gap(px(4.))
                                    .items_center()
                                    .text_color(theme.muted_foreground)
                                    .child("Ctrl+Enter")
                                    .child(div().flex_1())
                                    .child(
                                        Button::new("thread-reply-cancel")
                                            .xsmall()
                                            .ghost()
                                            .label("Cancel")
                                            .on_click(cx.listener(|this, _, window, cx| this.close_reply(window, cx))),
                                    )
                                    .child(
                                        Button::new("thread-reply-send")
                                            .xsmall()
                                            .primary()
                                            .label("Reply")
                                            .disabled(empty)
                                            .on_click(cx.listener(|this, _, window, cx| this.send_reply(window, cx))),
                                    ),
                            ),
                    )
                })
                .child(actions)
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
            .when_some(self.notice.clone(), |d, notice| {
                d.child(div().id("files-notice").test_support().text_xs().text_color(theme.danger).child(notice))
            })
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
        let annotate = {
            let this = this.clone();
            move |annotation: &DiffAnnotation, _: &mut Window, cx: &mut App| -> AnyElement {
                let Some(tab) = this.upgrade() else { return div().into_any_element() };
                annotation_element(&tab, annotation, cx)
            }
        };
        let add = {
            let this = this.clone();
            move |range: &DiffLineRange, window: &mut Window, cx: &mut App| {
                let range = range.clone();
                let _ = this.update(cx, |this, cx| this.open_composer(range, window, cx));
            }
        };
        let selecting = diff.read(cx).selected_lines().is_some();
        let body = Diff::new(&diff)
            .size_full()
            .decorations(self.decorations(cx))
            .annotations(self.annotations())
            .render_annotation(annotate)
            .when(selecting, |d| d.on_add_annotation(add))
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

/// The end line of `range` and its side: where GitHub anchors the comment.
fn end_of(range: &DiffLineRange) -> (DiffSide, usize) {
    if range.side() == range.end_side() {
        (range.side(), range.start().max(range.end()))
    } else {
        (range.end_side(), range.end())
    }
}

/// A draft for `range`. A range on one side is a multi-line comment; one
/// that crosses sides (a Unified selection over removed and added lines) is
/// sent as a comment on its last line.
fn draft_for(range: &DiffLineRange, body_md: String) -> DraftReviewComment {
    let (side, line) = end_of(range);
    let start = range.start().min(range.end());
    let start_line = (range.side() == range.end_side() && start < line).then_some(start as u32);
    DraftReviewComment {
        path: range.path().to_string(),
        side: match side {
            DiffSide::Original => fl_types::DiffSide::Left,
            DiffSide::Modified => fl_types::DiffSide::Right,
        },
        line: line as u32,
        start_line,
        body_md,
    }
}

/// A draft card (Remove) or the open box (Add to review / Cancel).
fn annotation_element(tab: &Entity<FilesTab>, annotation: &DiffAnnotation, cx: &mut App) -> AnyElement {
    let theme = cx.theme().clone();
    let weak = tab.downgrade();
    let card = v_flex()
        .mx(px(12.))
        .my(px(4.))
        .p(px(8.))
        .gap(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .text_xs();
    if annotation.id() == &gpui_kit::ElementId::from("draft-composer") {
        let Some((range, state)) = tab.read(cx).composer.clone() else { return div().into_any_element() };
        let empty = state.read(cx).value().trim().is_empty();
        let (_, line) = end_of(&range);
        let start = range.start().min(range.end());
        let place = if start < line { format!("Lines {start}\u{2013}{line}") } else { format!("Line {line}") };
        let (w1, w2, w3) = (weak.clone(), weak.clone(), weak);
        return card
            .key_context("Reply")
            .on_action(move |_: &CancelReply, window, cx| {
                let _ = w1.update(cx, |this, cx| this.close_composer(window, cx));
            })
            .child(Textarea::new(&state))
            .child(
                h_flex()
                    .gap(px(4.))
                    .items_center()
                    .text_color(theme.muted_foreground)
                    .child(format!("{place} \u{b7} Ctrl+Enter adds it to the review"))
                    .child(div().flex_1())
                    .child(Button::new("draft-cancel").xsmall().ghost().label("Cancel").on_click(move |_, window, cx| {
                        let _ = w2.update(cx, |this, cx| this.close_composer(window, cx));
                    }))
                    .child(
                        Button::new("draft-add")
                            .xsmall()
                            .primary()
                            .label("Add to review")
                            .disabled(empty)
                            .on_click(move |_, window, cx| {
                                let _ = w3.update(cx, |this, cx| this.add_draft(window, cx));
                            }),
                    ),
            )
            .into_any_element();
    }
    let ix = tab.read(cx).annotations().iter().position(|a| a.id() == annotation.id());
    let Some(draft) = ix.and_then(|ix| tab.read(cx).drafts.get(ix)) else { return div().into_any_element() };
    let ix = ix.unwrap_or_default();
    let place = match draft.start_line {
        Some(start) => format!("Pending comment on lines {start}\u{2013}{}", draft.line),
        None => "Pending comment".to_owned(),
    };
    card.border_dashed()
        .child(
            h_flex()
                .gap(px(4.))
                .items_center()
                .text_color(theme.muted_foreground)
                .child(format!("{place} (sent with your review)"))
                .child(div().flex_1())
                .child(Button::new(("draft-remove", ix)).xsmall().ghost().label("Remove").on_click(move |_, _, cx| {
                    let _ = weak.update(cx, |this, cx| this.remove_draft(ix, cx));
                })),
        )
        .child(div().text_color(theme.foreground).child(draft.body_md.clone()))
        .into_any_element()
}

#[cfg(test)]
mod tests;
