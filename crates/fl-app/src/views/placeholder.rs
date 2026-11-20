//! Empty view for areas G1 has not built yet.

use gpui_kit::component::{Icon, Sizable as _, v_flex};
use gpui_kit::{
    Context, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _, Window,
};

use crate::theme::ActivePalette as _;
use crate::view_ref::ViewRef;

pub struct Placeholder {
    view_ref: ViewRef,
    pub focus: FocusHandle,
}

impl Placeholder {
    pub fn new(view_ref: ViewRef, cx: &mut Context<Self>) -> Self {
        Self { view_ref, focus: cx.focus_handle() }
    }
}

impl Render for Placeholder {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .track_focus(&self.focus)
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .text_color(cx.palette().text_muted)
            .child(Icon::new(self.view_ref.icon()).large())
            .child(self.view_ref.title())
    }
}
