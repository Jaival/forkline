use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Focusable as _, TestAppContext};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::SignIn;
use crate::store::Store;
use crate::test_support::{NO_GITHUB, open_seeded, open_with, press, type_text, wait_for};
use crate::workspace::{OpenSignIn, Workspace};

/// A mock GitHub. Keep the runtime alive while the server is used.
fn github(mocks: Vec<Mock>) -> (tokio::runtime::Runtime, MockServer) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let server = rt.block_on(async {
        let server = MockServer::start().await;
        for mock in mocks {
            mock.mount(&server).await;
        }
        server
    });
    (rt, server)
}

fn user(token: &str, login: &str) -> Mock {
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", format!("Bearer {token}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": 42, "login": login })))
}

fn dialog(cx: &mut TestAppContext, ws: &Entity<Workspace>) -> Option<Entity<SignIn>> {
    cx.read(|cx| ws.read(cx).sign_in().cloned())
}

fn signed_in_as(cx: &mut TestAppContext, ws: &Entity<Workspace>) -> Option<String> {
    cx.read(|cx| {
        let footer = ws.read(cx).footer().read(cx);
        footer.auth().filter(|a| a.signed_in).map(|a| a.login.clone().unwrap_or_default())
    })
}

fn click(cx: &mut TestAppContext, handle: AnyWindowHandle, id: &'static str) {
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn signed_out(cx: &mut TestAppContext, dir: &std::path::Path, api: &str) -> (AnyWindowHandle, Entity<Workspace>) {
    let (handle, ws) = open_with(cx, dir, "", api, None);
    wait_for(cx, "the dialog opens", |cx| dialog(cx, &ws).is_some());
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    (handle, ws)
}

#[gpui_kit::test]
fn signed_in_opens_no_dialog(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, ws) = open_seeded(cx, dir.path(), "");
    wait_for(cx, "auth read", |cx| cx.read(|cx| ws.read(cx).footer().read(cx).auth().is_some()));
    assert!(dialog(cx, &ws).is_none());
}

#[gpui_kit::test]
fn signed_out_opens_the_dialog_until_dismissed(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = signed_out(cx, dir.path(), NO_GITHUB);
    // The token box holds the focus, so a paste goes there.
    let focused = cx
        .update_window(handle, |_, window, cx| {
            let d = ws.read(cx).sign_in().unwrap().read(cx);
            d.token_input().read(cx).focus_handle(cx).is_focused(window)
        })
        .unwrap();
    assert!(focused);

    click(cx, handle, "signin-close");
    assert!(dialog(cx, &ws).is_none());
    // Later refetches while signed out do not reopen it.
    crate::test_support::write(cx, "", &["sync"]);
    cx.run_until_parked();
    assert!(dialog(cx, &ws).is_none());

    // The footer link opens it again.
    click(cx, handle, "footer-sign-in");
    assert!(dialog(cx, &ws).is_some());
}

#[gpui_kit::test]
fn esc_closes_the_dialog_and_the_palette_opens_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (handle, ws) = signed_out(cx, dir.path(), NO_GITHUB);
    press(cx, handle, "escape");
    assert!(dialog(cx, &ws).is_none());

    cx.update_window(handle, |_, window, cx| window.dispatch_action(Box::new(OpenSignIn), cx)).unwrap();
    cx.run_until_parked();
    assert!(dialog(cx, &ws).is_some());
}

#[gpui_kit::test]
fn a_pasted_token_signs_in(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_rt, server) = github(vec![user("ghp_good", "octo")]);
    let (handle, ws) = signed_out(cx, dir.path(), &server.uri());
    type_text(cx, handle, "ghp_good");
    press(cx, handle, "enter");
    wait_for(cx, "the dialog closes", |cx| dialog(cx, &ws).is_none());
    wait_for(cx, "the footer reads the login", |cx| signed_in_as(cx, &ws).as_deref() == Some("octo"));
}

#[gpui_kit::test]
fn a_rejected_token_keeps_the_dialog_with_an_error(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let rejected = Mock::given(method("GET"))
        .and(path("/user"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({ "message": "Bad credentials" })));
    let (_rt, server) = github(vec![rejected]);
    let (handle, ws) = signed_out(cx, dir.path(), &server.uri());
    type_text(cx, handle, "ghp_bad");
    click(cx, handle, "signin-save");
    wait_for(cx, "the error shows", |cx| {
        dialog(cx, &ws).is_some_and(|d| cx.read(|cx| d.read(cx).message() == Some("GitHub rejected this token.")))
    });
    assert!(signed_in_as(cx, &ws).is_none());
}

#[gpui_kit::test]
fn a_rejected_token_reopens_the_dialog(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (_, ws) = open_seeded(cx, dir.path(), "");
    wait_for(cx, "signed in", |cx| signed_in_as(cx, &ws).is_some());
    assert!(dialog(cx, &ws).is_none());
    // What the engine does on a 401: drop the token and emit `sync`.
    cx.update(|cx| Store::global(cx).read(cx).sync().set_test_token(None));
    crate::test_support::write(cx, "", &["sync"]);
    wait_for(cx, "the dialog reopens", |cx| dialog(cx, &ws).is_some());
}

#[gpui_kit::test]
fn the_device_flow_shows_the_code_then_signs_in(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let code = Mock::given(method("POST")).and(path("/login/device/code")).respond_with(
        ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "dc", "user_code": "ABCD-1234", "verification_uri": "https://github.com/login/device",
            "expires_in": 900, "interval": 1
        })),
    );
    let token = Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "gho_new" })));
    let (_rt, server) = github(vec![code, token, user("gho_new", "octo")]);
    let (handle, ws) = signed_out(cx, dir.path(), &server.uri());

    click(cx, handle, "signin-device");
    wait_for(cx, "the code shows", |cx| {
        dialog(cx, &ws).is_some_and(|d| cx.read(|cx| d.read(cx).code().is_some_and(|c| c.user_code == "ABCD-1234")))
    });
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    let shown = cx.update_window(handle, |_, window, _| window.find_all("signin-code").len()).unwrap();
    assert_eq!(shown, 1, "the code is on screen");
    assert_eq!(cx.opened_url().as_deref(), Some("https://github.com/login/device"));

    cx.executor().advance_clock(Duration::from_secs(1));
    wait_for(cx, "the dialog closes", |cx| dialog(cx, &ws).is_none());
    wait_for(cx, "signed in", |cx| signed_in_as(cx, &ws).as_deref() == Some("octo"));
}
