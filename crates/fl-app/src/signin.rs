//! Sign-in dialog: the GitHub device flow, or a pasted personal access token.
//!
//! The workspace shows it as an overlay, like the palette. The footer asks
//! for it while signed out: at launch, and again when GitHub rejects the
//! token (`Unauthorized`; the engine then emits `sync`). The dialog never
//! closes itself: it emits [`SignInEvent`] and the workspace closes it and
//! gives focus back.

use std::time::Duration;

use fl_types::{AuthPoll, AuthState, DeviceCode};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Escape, Input, InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render, SharedString, Styled as _,
    Subscription, Task, TestSupportExt as _, Window, actions, div, px,
};

use crate::theme::ActivePalette as _;
use crate::store::Store;
use crate::theme::TextScale as _;

#[cfg(test)]
mod tests;

actions!(sign_in, [CloseSignIn]);

/// Classic token with the scopes sync needs (`fl_sync::auth::SCOPES`).
const NEW_TOKEN_URL: &str =
    "https://github.com/settings/tokens/new?scopes=repo,read:org,notifications&description=Forkline";

pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CloseSignIn, Some("SignIn"))]);
}

pub enum SignInEvent {
    /// "Not now", "Cancel" or `Esc`.
    Close,
    /// A token is stored; sync runs.
    Done,
}

impl EventEmitter<SignInEvent> for SignIn {}

pub struct SignIn {
    auth: AuthState,
    /// The device flow's code while the dialog waits for approval.
    code: Option<DeviceCode>,
    token: Entity<InputState>,
    /// A request is out (device code or token check).
    busy: bool,
    message: Option<SharedString>,
    focus: FocusHandle,
    _poll: Option<Task<()>>,
    _subscription: Subscription,
}

impl SignIn {
    pub fn new(auth: AuthState, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let token = cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("ghp_… or github_pat_…"));
        let subscription = cx.subscribe_in(&token, window, |this, _, ev: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                this.save_token(cx);
            }
        });
        let focus = cx.focus_handle();
        // Paste goes straight into the token box, unless only the dev
        // override can help (then there is no box).
        if auth.env_override {
            window.focus(&focus, cx);
        } else {
            token.update(cx, |token, cx| token.focus(window, cx));
        }
        Self { auth, code: None, token, busy: false, message: None, focus, _poll: None, _subscription: subscription }
    }

    fn close(&mut self, _: &CloseSignIn, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(SignInEvent::Close);
    }

    /// `Esc` in the token box closes the dialog too.
    fn escape(&mut self, _: &Escape, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(SignInEvent::Close);
    }

    /// Ask GitHub for a device code, show it, open the page, then poll.
    fn start_device(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.message = None;
        let store = Store::global(cx).read(cx);
        let engine = store.sync().clone();
        let task = store.run(cx, async move { engine.auth_start().await });
        cx.spawn(async move |this, cx| {
            let result = flatten(task.await);
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(code) => {
                        cx.open_url(&code.verification_uri);
                        this.poll(code.interval, cx);
                        this.code = Some(code);
                    }
                    Err(e) => this.message = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Poll at the server's interval until GitHub answers.
    fn poll(&mut self, interval: u32, cx: &mut Context<Self>) {
        self._poll = Some(cx.spawn(async move |this, cx| {
            let mut wait = interval;
            loop {
                cx.background_executor().timer(Duration::from_secs(u64::from(wait.max(1)))).await;
                let Ok(task) = this.update(cx, |_, cx| {
                    let store = Store::global(cx).read(cx);
                    let engine = store.sync().clone();
                    store.run(cx, async move { engine.auth_poll().await })
                }) else {
                    return;
                };
                let result = flatten(task.await);
                let next = this.update(cx, |this, cx| {
                    let stop = match result {
                        Ok(AuthPoll::Pending { interval }) => return Some(interval),
                        Ok(AuthPoll::Done { .. }) => {
                            cx.emit(SignInEvent::Done);
                            return None;
                        }
                        Ok(AuthPoll::Expired) => "The code expired. Start again.".to_owned(),
                        Ok(AuthPoll::Denied) => "Sign-in was cancelled on GitHub.".to_owned(),
                        Err(e) => e,
                    };
                    this.code = None;
                    this.message = Some(stop.into());
                    cx.notify();
                    None
                });
                match next {
                    Ok(Some(interval)) => wait = interval,
                    _ => return,
                }
            }
        }));
    }

    /// Check the pasted token with GitHub and store it.
    fn save_token(&mut self, cx: &mut Context<Self>) {
        let token = self.token.read(cx).value().trim().to_owned();
        if self.busy || token.is_empty() {
            return;
        }
        self.busy = true;
        self.message = None;
        let store = Store::global(cx).read(cx);
        let engine = store.sync().clone();
        let task = store.run(cx, async move { engine.sign_in_with_token(&token).await });
        cx.spawn(async move |this, cx| {
            let result = match task.await {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(fl_sync::Error::Unauthorized)) => Err("GitHub rejected this token.".to_owned()),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok(()) => cx.emit(SignInEvent::Done),
                    Err(e) => this.message = Some(e.into()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    #[cfg(test)]
    pub(crate) fn code(&self) -> Option<&DeviceCode> {
        self.code.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn token_input(&self) -> &Entity<InputState> {
        &self.token
    }
}

/// Engine result through `Store::run`, as display text on error.
fn flatten<T>(r: Result<fl_sync::Result<T>, tokio::task::JoinError>) -> Result<T, String> {
    match r {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

impl Render for SignIn {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.palette();
        let muted = theme.text_muted;
        let code_box = |code: &DeviceCode| {
            let user_code = code.user_code.clone();
            let url = code.verification_uri.clone();
            v_flex()
                .gap_3()
                .child("Enter this code on GitHub. The page opened in your browser.")
                .child(
                    h_flex()
                        .gap_3()
                        .items_center()
                        .child(
                            div()
                                .id("signin-code")
                                .test_support()
                                .px_3()
                                .py_2()
                                .rounded(px(4.))
                                .bg(theme.raised)
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_size(px(18.)).line_height(px(26.))
                                .child(code.user_code.clone()),
                        )
                        .child(
                            Button::new("signin-copy")
                                .small()
                                .ghost()
                                .label("Copy")
                                .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(user_code.clone()))),
                        ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .text_color(muted)
                        .child("Waiting for approval…")
                        .child(
                            Button::new("signin-reopen")
                                .small()
                                .link()
                                .label("Open the page again")
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                        ),
                )
        };
        let device = if !self.auth.client_id_configured {
            div()
                .text_color(muted)
                .child("This build has no GitHub OAuth client ID, so sign-in through the browser is not available.")
                .into_any_element()
        } else {
            v_flex()
                .gap_2()
                .child("Forkline keeps a local copy of your repositories, issues, pull requests and inbox.")
                .child(
                    Button::new("signin-device")
                        .primary()
                        .small()
                        .label("Sign in with GitHub")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, _, cx| this.start_device(cx))),
                )
                .into_any_element()
        };
        let paste = v_flex()
            .gap_2()
            .child(
                div()
                    .w_full()
                    .text_color(muted)
                    .child("Or paste a personal access token (classic) with the scopes repo, read:org and notifications."),
            )
            .child(
                h_flex().child(
                    Button::new("signin-new-token")
                        .small()
                        .link()
                        .label("Create one on GitHub")
                        .on_click(|_, _, cx| cx.open_url(NEW_TOKEN_URL)),
                ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(Input::new(&self.token).small().mask_toggle()))
                    .child(
                        Button::new("signin-save")
                            .small()
                            .label("Save token")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, _, cx| this.save_token(cx))),
                    ),
            );
        let body = if self.auth.env_override {
            // The env token comes first (`fl_sync::auth::token`), so a new
            // token would not be used.
            div()
                .child("GitHub rejected FORKLINE_GITHUB_TOKEN. Set it to a valid token, or unset it, and restart Forkline.")
                .into_any_element()
        } else if let Some(code) = &self.code {
            code_box(code).into_any_element()
        } else {
            v_flex().gap_4().child(device).child(paste).into_any_element()
        };
        v_flex()
            .id("signin")
            .key_context("SignIn")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::close))
            .on_action(cx.listener(Self::escape))
            .w(px(480.))
            .p_5()
            .gap_3()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.control)
            .text_color(theme.text)
            .text_body()
            .shadow_lg()
            .child(div().text_size(px(16.)).line_height(px(22.)).font_weight(FontWeight::SEMIBOLD).child("Sign in to GitHub"))
            .child(body)
            .when_some(self.message.clone(), |d, m| d.child(div().id("signin-message").text_color(theme.closed.fg).child(m)))
            .child(
                h_flex().justify_end().child(
                    Button::new("signin-close")
                        .small()
                        .ghost()
                        .label(if self.code.is_some() { "Cancel" } else { "Not now" })
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(SignInEvent::Close))),
                ),
            )
    }
}
