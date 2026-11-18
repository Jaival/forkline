//! Action bar of the reading pane and its keys: `A` approve, `R` reply (on
//! an open pull request the box is the review composer: Comment, Approve,
//! Request changes),
//! `X` close / reopen, `E` done, `H` snooze; Merge (button only: it asks
//! for a confirm).
//!
//! Approve, reply, close / reopen and merge go through the optimistic action
//! queue (`fl-sync`). Done is local state of an Inbox notification and is
//! also sent to GitHub (`NotificationDone`); snooze is local only (GitHub has
//! no snooze). The queue hides the row until it updates or the snooze ends.

use fl_db::queries::inbox;
use fl_types::{Action, IssueState, MergeMethod, ReviewEvent};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, TestSupportExt as _, Window, actions, div, px,
};
use rusqlite::Connection;

use super::pickers::{EditAssignees, EditLabels};
use super::{Detail, ReadingPane};
use crate::store::Store;

actions!(reading, [Approve, Reply, ToggleState, Done, Snooze, CancelReply]);

/// `H` snoozes for one day.
const SNOOZE_SECS: i64 = 86_400;

const METHODS: [(MergeMethod, &str); 3] = [
    (MergeMethod::Merge, "Merge commit"),
    (MergeMethod::Squash, "Squash and merge"),
    (MergeMethod::Rebase, "Rebase and merge"),
];

fn method_label(method: MergeMethod) -> &'static str {
    METHODS.iter().find(|(m, _)| *m == method).map_or("Merge", |(_, label)| label)
}

/// What the Merge button can do for the item on show.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum MergeState {
    /// Not an open pull request with details: no button.
    None,
    Ready,
    /// A disabled button and the reason.
    Blocked(&'static str),
}

impl ReadingPane {
    /// The open pull request on show, if any.
    fn open_pr(&self) -> Option<i64> {
        let pr = self.detail.as_ref().and_then(Detail::pr)?;
        (pr.row.state == IssueState::Open).then_some(pr.row.id)
    }

    /// The notification thread id of the Inbox row on show.
    fn notification(&self) -> Option<String> {
        self.inbox.then(|| self.item.as_ref().map(|row| row.key.clone())).flatten()
    }

    pub(crate) fn approve(&mut self, _: &Approve, _: &mut Window, cx: &mut Context<Self>) {
        let Some(pr_id) = self.open_pr() else { return };
        self.submit_review(pr_id, ReviewEvent::Approve, String::new(), cx);
    }

    /// Inline comments drafted in the Files tab, waiting for the review.
    pub(super) fn draft_count(&self, cx: &gpui_kit::App) -> usize {
        self.files.as_ref().map_or(0, |f| f.read(cx).drafts.len())
    }

    /// Send a review with the Files tab drafts. The drafts leave the tab; a
    /// rejected review puts them back.
    fn submit_review(&mut self, pr_id: i64, event: ReviewEvent, body_md: String, cx: &mut Context<Self>) {
        let files = self.files.clone();
        let comments = files.as_ref().map(|f| f.update(cx, |f, cx| f.take_drafts(cx))).unwrap_or_default();
        let back = comments.clone();
        self.enqueue_or(Action::SubmitReview { pr_id, event, body_md, comments }, cx, move |_, cx| {
            if let Some(files) = files {
                files.update(cx, |f, cx| f.restore_drafts(back, cx));
            }
        });
    }

    pub(super) fn merge_state(&self) -> MergeState {
        let Some(pr) = self.detail.as_ref().and_then(Detail::pr) else { return MergeState::None };
        if pr.row.state != IssueState::Open {
            return MergeState::None;
        }
        // The checks of the Tauri merge box; GitHub checks the rest.
        if pr.is_draft {
            MergeState::Blocked("Draft")
        } else if pr.mergeable.as_deref() == Some("CONFLICTING") {
            MergeState::Blocked("Has conflicts")
        } else if !pr.pending_fields.is_empty() {
            MergeState::Blocked("A change is waiting to sync")
        } else {
            MergeState::Ready
        }
    }

    /// `X`: close an open item, reopen a closed one. A merged PR has neither.
    pub(crate) fn toggle_state(&mut self, _: &ToggleState, _: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.detail.as_ref().map(Detail::row) else { return };
        let state = match row.state {
            IssueState::Open => IssueState::Closed,
            IssueState::Closed => IssueState::Open,
            IssueState::Merged => return,
        };
        let issue_id = row.id;
        self.enqueue(Action::SetState { issue_id, state }, cx);
    }

    /// A method picked from the Merge menu; the bar then asks to confirm.
    pub(super) fn pick_merge(&mut self, method: MergeMethod, cx: &mut Context<Self>) {
        self.merge_confirm = Some(method);
        cx.notify();
    }

    pub(super) fn confirm_merge(&mut self, cx: &mut Context<Self>) {
        let Some(method) = self.merge_confirm.take() else { return };
        cx.notify();
        if self.merge_state() != MergeState::Ready {
            return;
        }
        let Some(pr_id) = self.open_pr() else { return };
        self.enqueue(Action::Merge { pr_id, method }, cx);
    }

    /// Open the reply box under the item and focus it.
    pub(crate) fn reply(&mut self, _: &Reply, window: &mut Window, cx: &mut Context<Self>) {
        if self.detail.is_none() {
            return;
        }
        let state = match &self.reply {
            Some(state) => state.clone(),
            None => {
                let state = cx.new(|cx| {
                    let mut state = TextareaState::new(window, cx).placeholder("Reply");
                    state.set_auto_grow(3, 12, cx);
                    state
                });
                let subscription = cx.subscribe_in(&state, window, |this, _, ev: &InputEvent, window, cx| {
                    match ev {
                        InputEvent::PressEnter { secondary: true, .. } => this.send_reply(window, cx),
                        // Request changes needs text.
                        InputEvent::Change => cx.notify(),
                        _ => {}
                    }
                });
                self.reply = Some(state.clone());
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

    /// The text in the reply box, trimmed.
    fn reply_body(&self, cx: &Context<Self>) -> Option<String> {
        Some(self.reply.as_ref()?.read(cx).value().trim().to_owned())
    }

    /// `Ctrl+Enter` / Comment: a comment on the item. On an open PR with
    /// inline drafts it is a `COMMENT` review that carries them.
    fn send_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(body_md), Some(detail)) = (self.reply_body(cx), &self.detail) else { return };
        let issue_id = detail.row().id;
        if let Some(pr_id) = self.open_pr().filter(|_| self.draft_count(cx) > 0) {
            self.close_reply(window, cx);
            self.submit_review(pr_id, ReviewEvent::Comment, body_md, cx);
            return;
        }
        if body_md.is_empty() {
            return;
        }
        self.close_reply(window, cx);
        self.enqueue(Action::Comment { issue_id, body_md }, cx);
    }

    /// Approve or Request changes from the composer, with its text as the
    /// review body and the inline drafts. GitHub rejects Request changes with
    /// neither.
    pub(super) fn send_review(&mut self, event: ReviewEvent, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(body_md), Some(pr_id)) = (self.reply_body(cx), self.open_pr()) else { return };
        if body_md.is_empty() && event != ReviewEvent::Approve && self.draft_count(cx) == 0 {
            return;
        }
        self.close_reply(window, cx);
        self.submit_review(pr_id, event, body_md, cx);
    }

    pub(crate) fn cancel_reply(&mut self, _: &CancelReply, window: &mut Window, cx: &mut Context<Self>) {
        self.close_reply(window, cx);
    }

    /// Drop the reply box and give focus back to where `R` was pressed.
    fn close_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reply = None;
        self._reply_subscription = None;
        let focus = self.return_focus.take().unwrap_or_else(|| self.focus.clone());
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Done here and on GitHub: the local `done_at` hides the row at once,
    /// `NotificationDone` marks the thread done on GitHub. Both return on
    /// new activity.
    pub(crate) fn done(&mut self, _: &Done, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.notification() else { return };
        self.triage(inbox::set_done, cx);
        self.enqueue(Action::NotificationDone { id }, cx);
    }

    pub(crate) fn snooze(&mut self, _: &Snooze, _: &mut Window, cx: &mut Context<Self>) {
        self.triage(|c, id, now| inbox::snooze(c, id, now + SNOOZE_SECS), cx);
    }

    /// Write local triage state of the Inbox row; the queue then drops it.
    fn triage(
        &mut self,
        write: impl FnOnce(&Connection, &str, i64) -> rusqlite::Result<bool> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.notification() else { return };
        let db = Store::global(cx).read(cx).db().clone();
        let task = cx.background_spawn(async move { db.write(|c| write(c, &id, fl_sync::now())) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) => Store::emit_changed(&["inbox"], cx),
                Err(e) => {
                    this.notice = Some(e.to_string().into());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Queue `action`. It applies to the local rows at once; a rejected
    /// action shows its error in the bar.
    pub(super) fn enqueue(&mut self, action: Action, cx: &mut Context<Self>) {
        self.enqueue_or(action, cx, |_, _| {});
    }

    /// `enqueue`, then `on_error` when the queue rejects the action.
    fn enqueue_or(
        &mut self,
        action: Action,
        cx: &mut Context<Self>,
        on_error: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
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
                on_error(this, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// The buttons that apply to the item on show; `None` when none does.
    pub(super) fn action_bar(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let approve = self.open_pr().is_some();
        let reply = self.detail.is_some();
        let triage = self.notification().is_some();
        if !(approve || reply || triage) {
            return None;
        }
        let state_button = match self.detail.as_ref().map(|d| d.row().state) {
            Some(IssueState::Open) => Some(("action-close", "Close")),
            Some(IssueState::Closed) => Some(("action-reopen", "Reopen")),
            _ => None,
        };
        let merge = self.merge_element(cx);
        let theme = cx.theme();
        let button = |id: &'static str, label: &'static str, key: &'static str| {
            Button::new(id)
                .small()
                .ghost()
                .label(label)
                .child(div().text_xs().text_color(theme.muted_foreground).child(key))
        };
        Some(
            h_flex()
                .flex_none()
                .h(px(34.))
                .px(px(8.))
                .gap(px(2.))
                .items_center()
                .border_b_1()
                .border_color(theme.border)
                .when(approve, |d| {
                    d.child(
                        button("action-approve", "Approve", "A")
                            .on_click(cx.listener(|this, _, window, cx| this.approve(&Approve, window, cx))),
                    )
                })
                .when(reply, |d| {
                    d.child(
                        button("action-reply", "Reply", "R")
                            .on_click(cx.listener(|this, _, window, cx| this.reply(&Reply, window, cx))),
                    )
                })
                .when(reply, |d| {
                    d.child(
                        button("action-labels", "Labels", "L")
                            .on_click(cx.listener(|this, _, window, cx| this.edit_labels(&EditLabels, window, cx))),
                    )
                    .child(button("action-assignees", "Assignees", "").on_click(
                        cx.listener(|this, _, window, cx| this.edit_assignees(&EditAssignees, window, cx)),
                    ))
                })
                .when_some(state_button, |d, (id, label)| {
                    d.child(
                        button(id, label, "X")
                            .on_click(cx.listener(|this, _, window, cx| this.toggle_state(&ToggleState, window, cx))),
                    )
                })
                .when(triage, |d| {
                    d.child(
                        button("action-done", "Done", "E")
                            .on_click(cx.listener(|this, _, window, cx| this.done(&Done, window, cx))),
                    )
                    .child(
                        button("action-snooze", "Snooze", "H")
                            .on_click(cx.listener(|this, _, window, cx| this.snooze(&Snooze, window, cx))),
                    )
                })
                .children(merge)
                .child(div().flex_1())
                .when_some(self.notice.clone(), |d, notice: SharedString| {
                    d.child(div().truncate().min_w_0().text_xs().text_color(theme.danger).child(notice))
                })
                .into_any_element(),
        )
    }

    /// Merge: a menu of methods, then Confirm / Cancel; disabled with the
    /// reason when GitHub would refuse.
    fn merge_element(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = cx.theme();
        let el = match (self.merge_state(), self.merge_confirm) {
            (MergeState::None, _) => return None,
            (MergeState::Blocked(reason), _) => h_flex()
                .gap(px(6.))
                .items_center()
                .child(Button::new("action-merge").small().ghost().label("Merge").disabled(true))
                .child(div().text_xs().text_color(theme.muted_foreground).child(reason))
                .into_any_element(),
            (MergeState::Ready, Some(method)) => h_flex()
                .gap(px(2.))
                .items_center()
                .child(
                    Button::new("action-merge-confirm")
                        .small()
                        .primary()
                        .label(format!("Confirm: {}", method_label(method)))
                        .on_click(cx.listener(|this, _, _, cx| this.confirm_merge(cx))),
                )
                .child(
                    Button::new("action-merge-cancel")
                        .small()
                        .ghost()
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.merge_confirm = None;
                            cx.notify();
                        })),
                )
                .into_any_element(),
            (MergeState::Ready, None) => {
                let pane = cx.weak_entity();
                Button::new("action-merge")
                    .small()
                    .ghost()
                    .label("Merge")
                    .dropdown_caret(true)
                    .dropdown_menu(move |mut menu, _, _| {
                        for (method, label) in METHODS {
                            let pane = pane.clone();
                            menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                                let _ = pane.update(cx, |pane, cx| pane.pick_merge(method, cx));
                            }));
                        }
                        menu
                    })
                    .into_any_element()
            }
        };
        Some(el)
    }

    /// The reply box, when open. On an open pull request it also submits a
    /// review.
    pub(super) fn reply_box(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let state = self.reply.as_ref()?;
        let review = self.open_pr().is_some();
        let drafts = if review { self.draft_count(cx) } else { 0 };
        let empty = self.reply_body(cx).is_none_or(|b| b.is_empty()) && drafts == 0;
        let theme = cx.theme();
        Some(
            v_flex()
                .flex_none()
                .key_context("Reply")
                .on_action(cx.listener(Self::cancel_reply))
                .p(px(8.))
                .gap(px(6.))
                .border_t_1()
                .border_color(theme.border)
                .child(Textarea::new(state))
                .child(
                    h_flex()
                        .gap(px(8.))
                        .items_center()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Ctrl+Enter to send, Esc to cancel")
                        .when(drafts > 0, |d| {
                            let s = if drafts == 1 { "" } else { "s" };
                            d.child(
                                div()
                                    .id("review-drafts")
                                    .test_support()
                                    .text_color(theme.foreground)
                                    .child(format!("{drafts} inline comment{s} go with it")),
                            )
                        })
                        .child(div().flex_1())
                        .when(review, |d| {
                            d.child(
                                Button::new("review-request-changes")
                                    .small()
                                    .ghost()
                                    .label("Request changes")
                                    .disabled(empty)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.send_review(ReviewEvent::RequestChanges, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("review-approve")
                                    .small()
                                    .ghost()
                                    .label("Approve")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.send_review(ReviewEvent::Approve, window, cx)
                                    })),
                            )
                        })
                        .child(
                            Button::new("reply-send")
                                .small()
                                .primary()
                                .label("Comment")
                                .on_click(cx.listener(|this, _, window, cx| this.send_reply(window, cx))),
                        ),
                )
                .into_any_element(),
        )
    }
}
