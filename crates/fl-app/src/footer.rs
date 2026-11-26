//! Footer: sync status ("Synced 12 s ago", pending / failed actions, errors),
//! the failed-actions list with Retry / Discard, and Sign out (the account
//! menu in the sidebar and rail, and the palette, call [`Footer::sign_out`]).
//!
//! The footer also watches the sign-in state: it emits
//! [`FooterEvent::SignIn`] when it first reads "signed out" and whenever the
//! state goes from signed in to signed out (GitHub rejected the token). The
//! workspace then shows the sign-in dialog.

use std::time::Duration;

use fl_types::{ActionId, AuthState, DataChanged, FailedAction, SyncStatus};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, EventEmitter, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Task, TestSupportExt as _, Window,
    deferred, div, px,
};

use crate::store::Store;
use crate::theme::{ActivePalette as _, FOOTER_HEIGHT, TextScale as _, scaled};

/// The relative time ticks once a second; the status itself is refetched on
/// a `sync` change and every `REFETCH_EVERY` ticks.
const TICK: Duration = Duration::from_secs(1);
const REFETCH_EVERY: u32 = 10;

pub enum FooterEvent {
    /// Show the sign-in dialog.
    SignIn,
}

impl EventEmitter<FooterEvent> for Footer {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Popover {
    Failed,
}

pub struct Footer {
    status: Option<SyncStatus>,
    /// `None` until read, and always in a bench run (no sign-in there).
    auth: Option<AuthState>,
    /// Read while the failed list is open.
    failed: Vec<FailedAction>,
    popover: Option<Popover>,
    /// A Retry, Discard or Sign out that failed.
    notice: Option<SharedString>,
    /// The error toast is open (the alert icon toggles it).
    toast: bool,
    /// The errors last shown in the toast, kept while it fades out.
    toast_shown: Vec<SharedString>,
    _tick: Task<()>,
    _subscription: Subscription,
}

impl Footer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let store = Store::global(cx);
        let subscription = cx.subscribe(&store, |this, _, ev: &DataChanged, cx| {
            if ev.scopes.iter().any(|s| s == "sync") {
                this.refetch(cx);
            }
        });
        let tick = cx.spawn(async move |this, cx| {
            let mut n = 0u32;
            loop {
                cx.background_executor().timer(TICK).await;
                n += 1;
                let alive = this.update(cx, |this, cx| {
                    if n.is_multiple_of(REFETCH_EVERY) {
                        this.refetch(cx);
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        let mut this = Self {
            status: None,
            auth: None,
            failed: Vec::new(),
            popover: None,
            notice: None,
            toast: false,
            toast_shown: Vec::new(),
            _tick: tick,
            _subscription: subscription,
        };
        this.refetch(cx);
        this
    }

    /// Sign-in state, once read.
    pub fn auth(&self) -> Option<&AuthState> {
        self.auth.as_ref()
    }

    fn refetch(&mut self, cx: &mut Context<Self>) {
        let store = Store::global(cx).read(cx);
        let sync = store.sync().clone();
        // A bench run never signs in, and `auth_state` may start `gh`.
        let with_auth = !store.read_only();
        let with_failed = self.popover == Some(Popover::Failed);
        let task = store.run(cx, async move {
            let status = sync.status().await?;
            let auth = if with_auth { Some(sync.auth_state().await?) } else { None };
            let failed = if with_failed { sync.failed_actions().await? } else { Vec::new() };
            Ok::<_, fl_sync::Error>((status, auth, failed))
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok((status, auth, failed))) = task.await {
                let _ = this.update(cx, |this, cx| {
                    if status.failed_actions == 0 && this.popover == Some(Popover::Failed) {
                        this.popover = None;
                    }
                    this.status = Some(status);
                    this.failed = failed;
                    if let Some(auth) = auth {
                        this.set_auth(auth, cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn set_auth(&mut self, auth: AuthState, cx: &mut Context<Self>) {
        let was_signed_in = self.auth.as_ref().map(|a| a.signed_in);
        if !auth.signed_in && was_signed_in != Some(false) {
            cx.emit(FooterEvent::SignIn);
        }
        self.auth = Some(auth);
    }

    /// Forget the token. A failure shows in the footer.
    pub(crate) fn sign_out(&mut self, cx: &mut Context<Self>) {
        self.run(Op::SignOut, cx);
    }

    fn toggle(&mut self, popover: Popover, cx: &mut Context<Self>) {
        self.popover = (self.popover != Some(popover)).then_some(popover);
        self.notice = None;
        self.toast = false;
        if self.popover == Some(Popover::Failed) {
            self.refetch(cx);
        }
        cx.notify();
    }

    /// Run `op` on the engine; its `sync` change refreshes the footer. A
    /// failure shows in the footer.
    fn run(&mut self, op: Op, cx: &mut Context<Self>) {
        self.notice = None;
        let store = Store::global(cx).read(cx);
        let engine = store.sync().clone();
        let task = store.run(cx, async move {
            match op {
                Op::Retry(id) => engine.retry(id).await,
                Op::Discard(id) => engine.discard(id).await,
                Op::SignOut => engine.sign_out().await,
            }
        });
        cx.spawn(async move |this, cx| {
            let error = match task.await {
                Ok(Ok(())) => return,
                Ok(Err(e)) => e.to_string(),
                Err(e) => e.to_string(),
            };
            let _ = this.update(cx, |this, cx| {
                // An action the user ran: show its error at once.
                this.notice = Some(error.into());
                this.toast = true;
                this.popover = None;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// The sync error and the last failed action, if any.
    fn errors(&self) -> Vec<SharedString> {
        let sync = self.status.as_ref().and_then(|s| s.error.clone()).map(SharedString::from);
        sync.into_iter().chain(self.notice.clone()).collect()
    }

    fn toggle_toast(&mut self, cx: &mut Context<Self>) {
        self.toast = !self.toast;
        self.popover = None;
        cx.notify();
    }

    /// Close the toast. A failed action's error is gone after that; the sync
    /// error stays behind the icon until the next sync clears it.
    fn close_toast(&mut self, cx: &mut Context<Self>) {
        self.toast = false;
        self.notice = None;
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn failed_ids(&self) -> Vec<ActionId> {
        self.failed.iter().map(|f| f.id).collect()
    }

    fn failed_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = *cx.palette();
        let rows = self.failed.iter().enumerate().map(|(ix, f)| {
            let id = f.id;
            h_flex()
                .gap_2()
                .py(px(6.))
                .items_start()
                .when(ix > 0, |d| d.border_t_1().border_color(p.border))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().truncate().child(f.summary.clone()))
                        .child(div().truncate().text_color(p.closed.fg).child(f.error.clone())),
                )
                .child(
                    Button::new(("failed-retry", ix))
                        .xsmall()
                        .ghost()
                        .label("Retry")
                        .on_click(cx.listener(move |this, _, _, cx| this.run(Op::Retry(id), cx))),
                )
                .child(
                    Button::new(("failed-discard", ix))
                        .xsmall()
                        .ghost()
                        .label("Discard")
                        .on_click(cx.listener(move |this, _, _, cx| this.run(Op::Discard(id), cx))),
                )
        });
        popover_box("footer-failed-list", cx)
            .left(px(8.))
            .w(px(440.))
            .child(div().pb_1().text_color(p.text_muted).child("These changes did not reach GitHub and were undone."))
            .child(div().id("footer-failed-rows").max_h(px(260.)).overflow_y_scroll().children(rows))
            .into_any_element()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Retry(ActionId),
    Discard(ActionId),
    SignOut,
}

/// A box over the dock, just above the footer.
fn popover_box(id: &'static str, cx: &Context<Footer>) -> gpui_kit::Stateful<gpui_kit::Div> {
    let p = cx.palette();
    div()
        .id(id)
        .occlude()
        .absolute()
        .bottom(px(30.))
        .p_2()
        .rounded(px(6.))
        .border_1()
        .border_color(p.border_strong)
        .bg(p.control)
        .text_color(p.text)
        .text_small()
        .shadow_lg()
}

fn ago(secs: i64) -> String {
    match secs.max(0) {
        s if s < 60 => format!("{s} s"),
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 86_400 => format!("{} h", s / 3600),
        s => format!("{} d", s / 86_400),
    }
}

impl Render for Footer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *cx.palette();
        let errors = self.errors();
        let (synced, pending, failed, note) = match &self.status {
            None => ("Loading…".to_owned(), 0, 0, None),
            Some(s) => (
                // Until every pinned repo had its first deep sync, say how
                // far it got: lists fill in over minutes on a first run.
                if s.repos_synced < s.repos_pinned {
                    format!("Syncing repos: {} of {}", s.repos_synced, s.repos_pinned)
                } else {
                    s.last_synced_at.map_or("Not synced yet".to_owned(), |t| {
                        format!("Synced {} ago", ago(fl_sync::now() - t))
                    })
                },
                s.pending_actions,
                s.failed_actions,
                s.note.clone(),
            ),
        };
        let syncing = self.status.as_ref().is_some_and(|s| s.repos_synced < s.repos_pinned);
        let signed_out = self.auth.as_ref().is_some_and(|a| !a.signed_in);
        let (icon, icon_color) = match (!errors.is_empty(), signed_out, &self.status) {
            (true, _, _) => (IconName::TriangleAlert, p.closed.fg),
            (_, true, _) => (IconName::CircleUser, p.text_muted),
            (_, _, Some(s)) if s.last_synced_at.is_some() && !syncing => (IconName::CircleCheck, p.open.fg),
            _ => (IconName::Loader, p.text_muted),
        };
        let left = h_flex()
            .gap(px(8.))
            .items_center()
            .map(|d| {
                let icon = Icon::new(icon).with_size(px(12.)).text_color(icon_color);
                if errors.is_empty() {
                    return d.child(icon);
                }
                // The alert opens the error toast.
                let n = errors.len();
                d.child(
                    div()
                        .id("footer-error")
                        .test_support()
                        .flex()
                        .items_center()
                        .gap_1()
                        .px_1()
                        .rounded(px(4.))
                        .cursor_pointer()
                        .text_color(p.closed.fg)
                        .hover(|s| s.bg(p.hover))
                        .when(self.toast, |d| d.bg(p.hover))
                        .child(icon)
                        .child(if n == 1 { "1 error".to_owned() } else { format!("{n} errors") })
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_toast(cx))),
                )
            })
            .map(|d| {
                if signed_out {
                    d.child(
                        Button::new("footer-sign-in")
                            .xsmall()
                            .link()
                            .label("Sign in to sync")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(FooterEvent::SignIn))),
                    )
                } else {
                    d.child(synced)
                }
            })
            .when_some((pending > 0).then_some(pending), |el, n| el.child(format!("{n} pending")))
            .when_some((failed > 0).then_some(failed), |el, n| {
                el.child(
                    div()
                        .id("footer-failed")
                        .test_support()
                        .cursor_pointer()
                        .text_color(p.closed.fg)
                        .hover(|s| s.underline())
                        .child(format!("{n} failed"))
                        .on_click(cx.listener(|this, _, _, cx| this.toggle(Popover::Failed, cx))),
                )
            })
            .when_some(note, |el, n| {
                el.child(div().id("footer-note").test_support().min_w_0().truncate().text_color(p.text_faint).child(n))
            });
        let hints = ["Ctrl K commands", "j / k move", "Enter open"];
        let mut right = h_flex().gap(px(8.)).items_center().text_color(p.text_faint);
        for (ix, hint) in hints.into_iter().enumerate() {
            if ix > 0 {
                right = right.child("·");
            }
            right = right.child(hint);
        }
        let popover = match self.popover {
            Some(Popover::Failed) if !self.failed.is_empty() => Some(self.failed_list(cx)),
            _ => None,
        };
        // The toast fades in and, once closed (or its errors clear), out.
        let live = self.toast && !errors.is_empty();
        if live {
            self.toast_shown = errors;
        }
        let t = crate::motion::presence("footer-toast", live, crate::motion::MENU, window, cx);
        if t.is_none() {
            self.toast_shown.clear();
        }
        let toast = t.filter(|_| !self.toast_shown.is_empty()).map(|t| {
            let title = if self.toast_shown.len() == 1 { "Error" } else { "Errors" };
            let toast = crate::toast::Toast::error("footer-toast", title, self.toast_shown.clone())
                .on_close({
                    let this = cx.entity().downgrade();
                    move |_, cx| _ = this.update(cx, |this, cx| this.close_toast(cx))
                });
            div().absolute().left(px(8.)).bottom(px(30.)).child(crate::motion::surface(
                div().relative().child(toast).when(!live, |d| d.child(crate::motion::shield())),
                t,
                -6.,
            ))
        });
        div()
            .relative()
            .flex_none()
            .child(
                h_flex()
                    .id("footer")
                    .h(scaled(FOOTER_HEIGHT))
                    .px(px(14.))
                    .gap(px(8.))
                    .items_center()
                    .justify_between()
                    .bg(p.surface)
                    .border_t_1()
                    .border_color(p.border)
                    .text_small()
                    .text_color(p.text_muted)
                    .child(left)
                    .child(right),
            )
            .children(popover.map(|p| deferred(p).with_priority(1)))
            .children(toast.map(|t| deferred(t).with_priority(1)))
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::{AppContext as _, TestAppContext};
    use gpui_kit::test::TestWindowExt as _;

    use super::ago;
    use crate::test_support::{open_seeded, query_i64, wait_for};

    // Two comments that failed for good (their local rows were rolled back).
    const SEED: &str = r#"
        INSERT INTO users(id, login) VALUES (1, 'me');
        INSERT INTO repos(id, node_id, owner, name, updated_at) VALUES (1, 'R', 'o', 'a', 0);
        INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,author_id,created_at,updated_at)
          VALUES (1,'I',1,1,0,'open','an issue',1,0,10);
        INSERT INTO action_queue(id,kind,payload_json,target_table,target_id,state,attempts,next_attempt_at,last_error,created_at,summary) VALUES
          (7,'comment','{"kind":"comment","issue_id":1,"body_md":"first"}','issues',1,'failed',5,1,'GitHub 422: nope',1,'Comment on o/a#1'),
          (8,'comment','{"kind":"comment","issue_id":1,"body_md":"second"}','issues',1,'failed',5,2,'GitHub 422: nope',2,'Comment on o/a#1');"#;

    #[gpui_kit::test]
    fn retry_and_discard_failed_actions(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open_seeded(cx, dir.path(), SEED);
        let footer = cx.read(|cx| ws.read(cx).footer().clone());
        let click = |cx: &mut TestAppContext, id: gpui_kit::ElementId| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                window.click(id, cx);
                window.render_frame(cx);
            })
            .unwrap();
        };
        wait_for(cx, "2 failed", |cx| cx.read(|cx| footer.read(cx).status.as_ref().is_some_and(|s| s.failed_actions == 2)));
        click(cx, "footer-failed".into());
        wait_for(cx, "the list loads", |cx| cx.read(|cx| footer.read(cx).failed_ids() == [7, 8]));

        // Retry: back in the queue as pending, and applied locally again.
        click(cx, ("failed-retry", 0usize).into());
        wait_for(cx, "7 retried", |cx| cx.read(|cx| footer.read(cx).failed_ids() == [8]));
        assert_eq!(query_i64(cx, "SELECT COUNT(*) FROM action_queue WHERE id = 7 AND state = 'pending'"), 1);
        assert_eq!(query_i64(cx, "SELECT COUNT(*) FROM comments WHERE issue_id = 1 AND body_md = 'first'"), 1);

        // Discard the last one: gone, and the list closes.
        click(cx, ("failed-discard", 0usize).into());
        wait_for(cx, "8 discarded", |cx| cx.read(|cx| footer.read(cx).popover.is_none()));
        assert_eq!(query_i64(cx, "SELECT COUNT(*) FROM action_queue WHERE id = 8"), 0);
        assert!(cx.read(|cx| footer.read(cx).notice.is_none()));
    }

    /// An error is not in the footer line: the alert opens a toast with it,
    /// and Close dismisses it.
    #[gpui_kit::test]
    fn the_alert_opens_an_error_toast(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open_seeded(cx, dir.path(), SEED);
        let footer = cx.read(|cx| ws.read(cx).footer().clone());
        cx.update(|cx| {
            footer.update(cx, |f, cx| {
                f.notice = Some("GitHub 502: bad gateway".into());
                cx.notify();
            })
        });
        let frame = |cx: &mut TestAppContext, click: Option<&'static str>| {
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                if let Some(id) = click {
                    window.click(id, cx);
                    window.render_frame(cx);
                }
                window.try_find("footer-toast").is_some()
            })
            .unwrap()
        };
        assert!(!frame(cx, None), "closed until clicked");
        assert!(frame(cx, Some("footer-error")));
        assert!(!frame(cx, Some("footer-toast-close")));
        assert!(cx.read(|cx| footer.read(cx).notice.is_none()));
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("footer-error").is_none(), "no errors left");
        })
        .unwrap();
    }

    #[test]
    fn formats_relative_times() {
        assert_eq!(ago(12), "12 s");
        assert_eq!(ago(125), "2 min");
        assert_eq!(ago(7200), "2 h");
        assert_eq!(ago(-3), "0 s");
    }
}
