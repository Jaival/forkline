//! Action bar of the reading pane and its keys: `A` approve, `R` reply,
//! `E` done, `H` snooze.
//!
//! Approve and reply go through the optimistic action queue (`fl-sync`).
//! Done and snooze are local state of an Inbox notification (GitHub has no
//! snooze): the queue hides the row until it updates or the snooze ends.

use fl_db::queries::inbox;
use fl_types::{Action, IssueState, ReviewEvent};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window, actions, div, px,
};
use rusqlite::Connection;

use super::{Detail, ReadingPane};
use crate::store::Store;

actions!(reading, [Approve, Reply, Done, Snooze, CancelReply]);

/// `H` snoozes for one day.
const SNOOZE_SECS: i64 = 86_400;

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
        let action =
            Action::SubmitReview { pr_id, event: ReviewEvent::Approve, body_md: String::new(), comments: Vec::new() };
        self.enqueue(action, cx);
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
                    if let InputEvent::PressEnter { secondary: true, .. } = ev {
                        this.send_reply(window, cx);
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

    fn send_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(state), Some(detail)) = (&self.reply, &self.detail) else { return };
        let body_md = state.read(cx).value().trim().to_owned();
        if body_md.is_empty() {
            return;
        }
        let issue_id = detail.row().id;
        self.close_reply(window, cx);
        self.enqueue(Action::Comment { issue_id, body_md }, cx);
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

    pub(crate) fn done(&mut self, _: &Done, _: &mut Window, cx: &mut Context<Self>) {
        self.triage(inbox::set_done, cx);
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

    /// The buttons that apply to the item on show; `None` when none does.
    pub(super) fn action_bar(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let approve = self.open_pr().is_some();
        let reply = self.detail.is_some();
        let triage = self.notification().is_some();
        if !(approve || reply || triage) {
            return None;
        }
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
                .child(div().flex_1())
                .when_some(self.notice.clone(), |d, notice: SharedString| {
                    d.child(div().truncate().min_w_0().text_xs().text_color(theme.danger).child(notice))
                })
                .into_any_element(),
        )
    }

    /// The reply box, when open.
    pub(super) fn reply_box(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let state = self.reply.as_ref()?;
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
                        .child(div().flex_1())
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
