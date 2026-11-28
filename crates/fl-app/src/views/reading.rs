//! Reading pane: one issue or pull request. It sits beside a queue (the
//! selected row) and fills a `ViewRef::Issue` / `ViewRef::Pr` tab.
//!
//! The header shows at once from the queue row; the details are read from
//! SQLite on a background thread. Opening an unread item sets
//! `last_viewed_at`; the value from before is kept while the item is on show
//! and marks what is new (the conversation divider, the tab dots).

mod action_bar;
mod conversation;
mod files;
mod header;
mod pickers;
mod tabs;
#[cfg(test)]
mod tests;

use fl_db::queries::{issue, pr, repo};
use fl_types::{DataChanged, IssueDetail, IssueId, IssueRow, IssueThread, MergeMethod, PrDetail, QueueRow, Timestamp, User};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::input::TextareaState;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Subscription, TestSupportExt as _, Window, actions, div, px,
};
use rusqlite::Connection;

pub use self::action_bar::{Approve, CancelReply, Done, Reply, Snooze, ToggleState};
pub use self::pickers::{ClosePicker, EditAssignees, EditLabels};
use self::conversation::Conversation;
use self::files::FilesTab;
use super::Built;
use crate::bench;
use crate::drafts::Drafts;
use crate::store::Store;
use crate::theme::{ActivePalette as _, TextScale as _};

/// Everything the pane shows for one item.
#[expect(clippy::large_enum_variant, reason = "one value per pane")]
pub(super) enum Detail {
    Issue { detail: IssueDetail, thread: IssueThread },
    Pr(PrDetail),
}

impl Detail {
    pub(super) fn row(&self) -> &IssueRow {
        match self {
            Self::Issue { detail, .. } => &detail.row,
            Self::Pr(pr) => &pr.row,
        }
    }

    pub(super) fn pr(&self) -> Option<&PrDetail> {
        match self {
            Self::Issue { .. } => None,
            Self::Pr(pr) => Some(pr),
        }
    }
}

/// The item, its assignees, for a PR the merge method last sent in its
/// repository, and the last view.
type Read = (Detail, Vec<User>, Option<MergeMethod>, Seen);

/// When the viewer last opened the item, and who the viewer is.
#[derive(Clone, Debug, Default)]
pub(super) struct Seen {
    /// `None`: never opened, so nothing counts as new.
    pub at: Option<Timestamp>,
    pub viewer: Option<String>,
}

impl Seen {
    /// Made after the last view by someone other than the viewer.
    pub(super) fn is_new(&self, author: Option<&str>, at: Option<Timestamp>) -> bool {
        matches!((self.at, at), (Some(seen), Some(at)) if at > seen)
            && (author.is_none() || author != self.viewer.as_deref())
    }
}

fn read_detail(conn: &Connection, id: IssueId, is_pr: bool) -> rusqlite::Result<Option<Read>> {
    // A pull request without `pr_details` (not synced in depth yet) shows as
    // its conversation alone.
    let pr = if is_pr { pr::get(conn, id)? } else { None };
    let detail = match pr {
        Some(pr) => Detail::Pr(pr),
        None => {
            let Some(detail) = issue::get(conn, id)? else { return Ok(None) };
            Detail::Issue { detail, thread: issue::thread(conn, id)? }
        }
    };
    let merge_method = match &detail {
        Detail::Pr(pr) => repo::merge_method(conn, pr.row.repo_id)?,
        Detail::Issue { .. } => None,
    };
    let (at, viewer) = issue::last_view(conn, id)?;
    Ok(Some((detail, issue::assignees(conn, id)?, merge_method, Seen { at, viewer })))
}

/// Tabs of a pull request. An issue shows the conversation only.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PaneTab {
    Conversation,
    Files,
    Commits,
    Checks,
}

impl PaneTab {
    const ALL: [Self; 4] = [Self::Conversation, Self::Files, Self::Commits, Self::Checks];
}

actions!(reading, [NextFile, PrevFile, ToggleViewed, ShowFiles]);

pub struct ReadingPane {
    /// The selected queue row: the header until the details arrive, and all
    /// there is for a row without a cached issue or PR (a release).
    pub(super) item: Option<QueueRow>,
    /// `(issue id, is_pr)` of the item on show.
    target: Option<(IssueId, bool)>,
    pub(super) detail: Option<Detail>,
    /// Assignees of the item on show.
    pub(super) assignees: Vec<User>,
    /// The label or assignee picker, while open.
    picker: Option<pickers::Picker>,
    /// The target is not in the local database.
    missing: bool,
    /// The last view before this one: read once per item, so marking the
    /// item viewed does not move what counts as new.
    seen: Option<Seen>,
    pub(super) tab: PaneTab,
    conversation: Conversation,
    /// The Files tab of the PR on show; built when the tab first opens.
    pub(super) files: Option<Entity<FilesTab>>,
    /// Bumped when a read in flight turns stale.
    generation: u32,
    /// Beside the Inbox queue: the item is a notification, so Done and
    /// Snooze apply.
    inbox: bool,
    /// The reply box, while open.
    pub(super) reply: Option<Entity<TextareaState>>,
    _reply_subscription: Option<Subscription>,
    /// Focus before `R`; it returns there when the reply box closes.
    return_focus: Option<FocusHandle>,
    /// The last action error, shown in the action bar.
    pub(super) notice: Option<SharedString>,
    /// The merge method picked, waiting for Confirm.
    pub(super) merge_confirm: Option<MergeMethod>,
    /// What the Merge button offers: the method last sent in the PR's repo,
    /// else a merge commit.
    pub(super) merge_method: MergeMethod,
    pub focus: FocusHandle,
    _store: Subscription,
    /// The Files tab label and the composer count the PR's drafts.
    _drafts: Subscription,
}

impl ReadingPane {
    /// `inbox`: the pane sits beside the Inbox queue.
    pub fn new(inbox: bool, cx: &mut Context<Self>) -> Self {
        let subscription = cx.subscribe(&Store::global(cx), |this, _, ev: &DataChanged, cx| {
            if ev.scopes.iter().any(|s| matches!(s.as_str(), "issues" | "pr")) {
                this.load(cx);
            }
        });
        Self {
            item: None,
            target: None,
            detail: None,
            assignees: Vec::new(),
            picker: None,
            missing: false,
            seen: None,
            tab: PaneTab::Conversation,
            conversation: Conversation::new(),
            files: None,
            generation: 0,
            inbox,
            reply: None,
            _reply_subscription: None,
            return_focus: None,
            notice: None,
            merge_confirm: None,
            merge_method: MergeMethod::Merge,
            focus: cx.focus_handle(),
            _store: subscription,
            _drafts: cx.observe(&Drafts::global(cx), |_, _, cx| cx.notify()),
        }
    }

    /// The view of a `ViewRef::Issue` / `ViewRef::Pr` tab.
    pub fn build(id: IssueId, is_pr: bool, cx: &mut App) -> Built {
        let view = cx.new(|cx| {
            let mut this = Self::new(false, cx);
            this.show(Some((id, is_pr)), cx);
            this
        });
        let focus = view.read(cx).focus.clone();
        Built { view: view.into(), focus }
    }

    /// Show the selected queue row.
    pub fn set_item(&mut self, item: Option<QueueRow>, cx: &mut Context<Self>) {
        let target = item.as_ref().and_then(|row| Some((row.issue_id?, row.is_pr)));
        self.item = item;
        self.show(target, cx);
        cx.notify();
    }

    fn show(&mut self, target: Option<(IssueId, bool)>, cx: &mut Context<Self>) {
        if self.target == target {
            return;
        }
        self.target = target;
        // A draft belongs to the item it was written for. The box held the
        // focus only if the user clicked into the pane; then it stays there.
        self.reply = None;
        self._reply_subscription = None;
        self.return_focus = None;
        self.notice = None;
        self.merge_confirm = None;
        self.picker = None;
        self.detail = None;
        self.assignees.clear();
        self.missing = false;
        self.seen = None;
        self.tab = PaneTab::Conversation;
        self.files = None;
        self.conversation.clear();
        self.generation += 1;
        self.load(cx);
    }

    /// Read the target again. The same item keeps its tab and scroll position.
    fn load(&mut self, cx: &mut Context<Self>) {
        let Some((id, is_pr)) = self.target else { return };
        self.generation += 1;
        let generation = self.generation;
        let task = Store::global(cx).read(cx).read(cx, move |c| read_detail(c, id, is_pr));
        cx.spawn(async move |this, cx| {
            let detail = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                let Ok(detail) = detail else { return };
                let (detail, assignees, merge_method) = match detail {
                    Some((detail, assignees, method, seen)) => {
                        this.seen.get_or_insert(seen);
                        (Some(detail), assignees, method)
                    }
                    None => (None, Vec::new(), None),
                };
                this.assignees = assignees;
                this.merge_method = merge_method.unwrap_or(MergeMethod::Merge);
                this.missing = detail.is_none();
                if let Some(detail) = &detail {
                    this.conversation.set(detail, this.seen.as_ref().unwrap_or(&Seen::default()), cx);
                    if let (Some(files), Some(pr)) = (&this.files, detail.pr()) {
                        let checks = pr.checks.clone();
                        files.update(cx, |files, cx| files.set_checks(checks, cx));
                    }
                    if detail.row().unread {
                        this.mark_viewed(id, is_pr, cx);
                    }
                }
                this.detail = detail;
                cx.notify();
            });
        })
        .detach();
    }

    /// Clear the unread mark (local only), then tell the lists.
    fn mark_viewed(&self, id: IssueId, is_pr: bool, cx: &mut Context<Self>) {
        let store = Store::global(cx).read(cx);
        // A bench run leaves its database as it found it.
        if store.read_only() {
            return;
        }
        let db = store.db().clone();
        let write = cx.background_spawn(async move {
            db.write(|c| {
                if is_pr { pr::mark_viewed(c, id, fl_sync::now()) } else { issue::mark_viewed(c, id, fl_sync::now()) }
            })
        });
        cx.spawn(async move |_, cx| {
            if write.await.is_ok() {
                cx.update(|cx| Store::emit_changed(&["issues"], cx));
            }
        })
        .detach();
    }

    pub(super) fn set_tab(&mut self, tab: PaneTab, cx: &mut Context<Self>) {
        self.tab = tab;
        if tab == PaneTab::Files
            && self.files.is_none()
            && let Some(pr) = self.detail.as_ref().and_then(Detail::pr)
        {
            let (id, checks) = (pr.row.id, pr.checks.clone());
            self.files = Some(cx.new(|cx| FilesTab::new(id, checks, cx)));
        }
        cx.notify();
    }

    /// The Files tab, while it is on show.
    fn files_on_show(&self) -> Option<Entity<FilesTab>> {
        (self.tab == PaneTab::Files).then(|| self.files.clone()).flatten()
    }

    pub(crate) fn next_file(&mut self, _: &NextFile, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(files) = self.files_on_show() {
            files.update(cx, |files, cx| files.step(true, cx));
        }
    }

    pub(crate) fn prev_file(&mut self, _: &PrevFile, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(files) = self.files_on_show() {
            files.update(cx, |files, cx| files.step(false, cx));
        }
    }

    pub(crate) fn toggle_viewed(&mut self, _: &ToggleViewed, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(files) = self.files_on_show() {
            files.update(cx, |files, cx| files.toggle_current(cx));
        }
    }

    /// Open the Files tab of the PR on show (no key yet; the bench uses it).
    pub(crate) fn show_files(&mut self, _: &ShowFiles, _: &mut Window, cx: &mut Context<Self>) {
        if self.detail.as_ref().and_then(Detail::pr).is_some() {
            self.set_tab(PaneTab::Files, cx);
        }
    }

    fn tab_bar(&self, pr: &PrDetail, cx: &Context<Self>) -> impl IntoElement + use<> {
        let p = *cx.palette();
        let passed = pr
            .checks
            .iter()
            .filter(|c| matches!(c.conclusion.as_deref(), Some("SUCCESS" | "NEUTRAL" | "SKIPPED")))
            .count();
        let failed = pr
            .checks
            .iter()
            .any(|c| matches!(c.conclusion.as_deref(), Some("FAILURE" | "TIMED_OUT" | "CANCELLED")));
        let checks_tone = if pr.checks.is_empty() || passed == pr.checks.len() {
            None
        } else if failed {
            Some(p.closed)
        } else {
            Some(p.pending)
        };
        let files = self.files.as_ref().and_then(|f| f.read(cx).file_count()).unwrap_or(pr.changed_files as usize);
        let files_count = match self.draft_count(cx) {
            // Git's count once read; GitHub's until then.
            0 => files.to_string(),
            n => format!("{files} \u{b7} {n} pending"),
        };
        let checks_count =
            if checks_tone.is_some() { format!("{passed}/{}", pr.checks.len()) } else { pr.checks.len().to_string() };
        // A dot: the tab holds something newer than the last view.
        let seen = self.seen.clone().unwrap_or_default();
        let new_commits = pr.commits.iter().any(|c| seen.is_new(c.author.as_deref(), c.authored_at));
        let new_checks = pr.checks.iter().any(|c| seen.is_new(None, c.at));
        let tabs = [
            ("Conversation", None, None, false),
            ("Files", Some(files_count), None, false),
            ("Commits", Some(pr.commits.len().to_string()), None, new_commits),
            ("Checks", Some(checks_count), checks_tone, new_checks),
        ];
        let selected = PaneTab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
        TabBar::new("reading-tabs")
            .underline()
            .px(px(16.))
            .border_b_1()
            .border_color(p.border)
            .selected_index(selected)
            .children(tabs.into_iter().map(|(label, count, tone, new)| {
                Tab::new().label(label).when_some(count, |tab, count| {
                    let (bg, fg) = tone.map_or((p.raised, p.text_muted), |t| (t.fill, t.on_fill));
                    tab.suffix(
                        h_flex()
                            .ml(px(6.))
                            .gap(px(5.))
                            .child(div().px(px(5.)).rounded(px(8.)).bg(bg).text_color(fg).text_label().child(count))
                            .when(new, |d| {
                                d.child(div().id(SharedString::from(format!("new-{label}"))).test_support().size(px(6.)).rounded_full().bg(p.accent))
                            }),
                    )
                })
            }))
            .on_click(cx.listener(|this, ix: &usize, _, cx| this.set_tab(PaneTab::ALL[*ix], cx)))
    }

    fn header_extra(&self) -> header::Extra<'_> {
        header::Extra {
            assignees: &self.assignees,
            labels_pending: self.field_pending(pickers::PickerKind::Labels),
            assignees_pending: self.field_pending(pickers::PickerKind::Assignees),
        }
    }

    fn message(&self, text: &'static str, cx: &App) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.palette().text_muted)
            .child(text)
            .into_any_element()
    }
}

impl Render for ReadingPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = v_flex()
            .key_context("Reading")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::approve))
            .on_action(cx.listener(Self::reply))
            .on_action(cx.listener(Self::toggle_state))
            .on_action(cx.listener(Self::edit_labels))
            .on_action(cx.listener(Self::edit_assignees))
            .on_action(cx.listener(Self::done))
            .on_action(cx.listener(Self::snooze))
            .on_action(cx.listener(Self::next_file))
            .on_action(cx.listener(Self::prev_file))
            .on_action(cx.listener(Self::toggle_viewed))
            .on_action(cx.listener(Self::show_files))
            .size_full();
        if self.item.is_none() && self.target.is_none() {
            return root.child(self.message("Select an item", cx));
        }
        let root = root
            .child(header::render(self.item.as_ref(), self.detail.as_ref(), self.header_extra(), cx))
            .children(self.action_bar(cx))
            .children(self.picker_box(cx));
        let Some(detail) = &self.detail else {
            let text = match (self.target, self.missing) {
                (None, _) => "This notification has no cached issue or pull request.",
                (Some(_), true) => "This item is not in the local cache.",
                // Loading takes a frame or two; show no text that would flash.
                (Some(_), false) => "",
            };
            return root.child(self.message(text, cx));
        };
        bench::ready("item", window, cx);
        let body = match (self.tab, detail.pr()) {
            (PaneTab::Files, Some(_)) => match &self.files {
                Some(files) => files.clone().into_any_element(),
                None => self.message("", cx),
            },
            (PaneTab::Commits, Some(pr)) => tabs::commits(pr, cx),
            (PaneTab::Checks, Some(pr)) => tabs::checks(pr, cx),
            _ => self.conversation.render(cx),
        };
        root.children(detail.pr().map(|pr| self.tab_bar(pr, cx)))
            .child(div().flex_1().min_h_0().child(body))
            .children(self.reply_box(cx))
    }
}
