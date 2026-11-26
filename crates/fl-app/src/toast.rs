//! Toast: a small card that floats over the app with a title, one or more
//! messages, Copy and Close. The footer shows errors in one (click its
//! alert icon); the caller places it and decides when it shows.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, ClipboardItem, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

use crate::theme::{ActivePalette as _, TextScale as _};

type OnClose = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Toast {
    id: &'static str,
    title: SharedString,
    messages: Vec<SharedString>,
    on_close: Option<OnClose>,
}

impl Toast {
    /// An error toast. Its Copy and Close buttons are `{id}-copy` and
    /// `{id}-close`.
    pub fn error(id: &'static str, title: impl Into<SharedString>, messages: Vec<SharedString>) -> Self {
        Self { id, title: title.into(), messages, on_close: None }
    }

    pub fn on_close(mut self, f: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
}

impl RenderOnce for Toast {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *cx.palette();
        let name = self.id;
        let text = self.messages.join("\n");
        let header = h_flex()
            .gap_2()
            .items_center()
            .child(Icon::new(IconName::TriangleAlert).with_size(px(14.)).text_color(p.closed.fg))
            .child(div().flex_1().min_w_0().truncate().text_color(p.text).child(self.title))
            .child(
                Button::new(SharedString::from(format!("{name}-copy")))
                    .xsmall()
                    .ghost()
                    .icon(IconName::Copy)
                    .tooltip("Copy")
                    .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))),
            )
            .when_some(self.on_close, |d, f| {
                d.child(
                    Button::new(SharedString::from(format!("{name}-close")))
                        .xsmall()
                        .ghost()
                        .icon(IconName::Close)
                        .tooltip("Close")
                        .on_click(move |_, window, cx| f(window, cx)),
                )
            });
        let body = v_flex()
            .id(SharedString::from(format!("{name}-body")))
            .gap_1()
            .max_h(px(200.))
            .overflow_y_scroll()
            .text_color(p.text_muted)
            .children(self.messages.into_iter().map(|m| div().child(m)));
        h_flex()
            .id(self.id)
            .test_support()
            .occlude()
            .w(px(380.))
            .rounded(px(6.))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.control)
            .text_small()
            .shadow_lg()
            .overflow_hidden()
            // The danger stripe.
            .child(div().w(px(3.)).flex_none().self_stretch().bg(p.closed.fg))
            .child(v_flex().flex_1().min_w_0().gap_1().p_2().child(header).child(body))
    }
}
