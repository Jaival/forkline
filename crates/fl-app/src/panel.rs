//! One dock tab. Holds a [`ViewRef`] and the view built for it; the `ViewRef`
//! is what the saved layout records, so a restored tab rebuilds the same view.

use gpui_kit::component::dock::{
    BasePanel, Panel, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window, div,
};

use crate::view_ref::ViewRef;
use crate::views;

pub const PANEL_NAME: &str = "forkline.view";

pub struct ViewPanel {
    view_ref: ViewRef,
    content: AnyView,
    focus_handle: FocusHandle,
}

impl ViewPanel {
    pub fn new(view_ref: ViewRef, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let built = views::build(&view_ref, window, cx);
        cx.new(|_| Self { content: built.view, view_ref, focus_handle: built.focus })
    }

    pub(crate) fn content(&self) -> &AnyView {
        &self.content
    }

    pub fn view_ref(&self) -> &ViewRef {
        &self.view_ref
    }
}

/// Lets the dock rebuild saved tabs. Call once at startup.
pub fn init(cx: &mut App) {
    register_panel(cx, PANEL_NAME, |ctx, window, cx| {
        let view_ref = match &ctx.state().info {
            PanelInfo::Panel(json) => serde_json::from_value(json.clone()).unwrap_or(ViewRef::Home),
            _ => ViewRef::Home,
        };
        panel_handle(ViewPanel::new(view_ref, window, cx))
    });
}

impl BasePanel for ViewPanel {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }

    fn dump(&self, _: &App) -> PanelState {
        let mut state = PanelState::new(PANEL_NAME);
        state.info = PanelInfo::panel(serde_json::to_value(&self.view_ref).unwrap_or_default());
        state
    }
}

impl Panel for ViewPanel {
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(self.view_ref.title())
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_1p5()
            .child(Icon::new(self.view_ref.icon()).small())
            .child(self.view_ref.title())
    }

    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl EventEmitter<PanelEvent> for ViewPanel {}

impl Focusable for ViewPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ViewPanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.content.clone())
    }
}
