//! The client-drawn title bar (the kit's `TitleBar`): the logo, the app
//! name, the command field (opens the `Ctrl+K` palette) and the kit's window
//! buttons. The window must use `TitleBar::window_options` (`main.rs`).
//!
//! On Windows the empty bar is a caption area (drag, double-click to
//! maximise, Snap); the command field occludes it, so it takes clicks.

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, Sizable as _, TitleBar, h_flex};
use gpui_kit::{
    App, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, div, px, svg,
};

use crate::theme::{ActivePalette as _, TITLE_BAR_HEIGHT, TextScale as _, scaled};
use crate::workspace::OpenPalette;

pub fn render(cx: &App) -> impl IntoElement + use<> {
    let p = cx.palette();
    // The two-colour logo as two one-colour layers, so it follows the theme.
    let logo = div()
        .relative()
        .size(px(18.))
        .flex_none()
        .child(svg().path("logo/stem.svg").absolute().size_full().text_color(p.text_strong))
        .child(svg().path("logo/branch.svg").absolute().size_full().text_color(p.accent));
    let name = h_flex()
        .flex_1()
        .flex_basis(px(0.))
        .min_w_0()
        .gap(px(8.))
        .items_center()
        .child(logo)
        .child(div().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD).text_color(p.text_strong).child("Forkline"));
    let command = h_flex()
        .id("titlebar-command")
        .test_support()
        .occlude()
        .flex_shrink_1()
        .w(px(420.))
        .min_w(px(160.))
        .h(scaled(28.))
        .px(px(10.))
        .gap(px(8.))
        .items_center()
        .rounded(px(6.))
        .border_1()
        .border_color(p.border)
        .text_small()
        .text_color(p.text_muted)
        .cursor_pointer()
        .child(Icon::new(IconName::Search).with_size(px(14.)))
        .child(div().flex_1().min_w_0().truncate().child("Search or run a command"))
        .child(div().text_color(p.text_faint).child("Ctrl K"))
        .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenPalette), cx));
    let command = crate::motion::hover_bg(command, p.control, Some(p.raised));
    TitleBar::new()
        .h(scaled(TITLE_BAR_HEIGHT))
        .pl(px(14.))
        .bg(p.surface)
        .border_color(p.border)
        .text_body()
        .child(name)
        .child(command)
        // Balances the name, so the field sits in the middle.
        .child(div().flex_1().flex_basis(px(0.)).min_w(px(8.)))
}

#[cfg(test)]
mod tests {
    use gpui_kit::{AppContext as _, TestAppContext};
    use gpui_kit::test::TestWindowExt as _;

    use crate::test_support::open;

    #[gpui_kit::test]
    fn the_command_field_opens_the_palette(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("titlebar-command", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.read(|cx| assert!(ws.read(cx).palette().is_some()));
    }
}
