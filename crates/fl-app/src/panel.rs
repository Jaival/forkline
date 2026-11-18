//! One dock tab. Holds a [`ViewRef`] and the view built for it; the `ViewRef`
//! is what the saved layout records, so a restored tab rebuilds the same view.

use gpui_kit::component::dock::{
    BasePanel, Panel, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use fl_db::queries::repo;
use fl_types::DataChanged;
use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _, Subscription,
    Window, div,
};

use crate::store::Store;
use crate::view_ref::ViewRef;
use crate::views;
use crate::workspace::SetViewRef;

pub const PANEL_NAME: &str = "forkline.view";

pub struct ViewPanel {
    view_ref: ViewRef,
    /// `owner/name` of the tab's repo, if it has one and the cache knows it.
    /// The `ViewRef` holds only the id, so the title looks the name up.
    repo_name: Option<String>,
    content: AnyView,
    focus_handle: FocusHandle,
    _subscription: Subscription,
}

impl ViewPanel {
    pub fn new(view_ref: ViewRef, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let built = views::build(&view_ref, window, cx);
        cx.new(|cx| {
            let subscription = cx.subscribe(&Store::global(cx), |this: &mut Self, _, ev: &DataChanged, cx| {
                if this.view_ref.repo_id().is_some() && ev.scopes.iter().any(|s| s == "repos") {
                    this.load_repo_name(cx);
                    cx.notify();
                }
            });
            let mut this = Self {
                content: built.view,
                view_ref,
                repo_name: None,
                focus_handle: built.focus,
                _subscription: subscription,
            };
            this.load_repo_name(cx);
            this
        })
    }

    /// Read synchronously: one row by key, and the tab title shows it on the
    /// first frame.
    fn load_repo_name(&mut self, cx: &App) {
        self.repo_name = self
            .view_ref
            .repo_id()
            .and_then(|id| Store::global(cx).read(cx).db().read(|c| repo::full_name(c, id)).ok().flatten());
    }

    pub fn title(&self) -> SharedString {
        self.view_ref.title_with(self.repo_name.as_deref())
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
        Some(ViewPanel::title(self))
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap_1p5()
            .child(Icon::new(self.view_ref.icon()).small())
            .child(ViewPanel::title(self))
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .on_action(cx.listener(|this, a: &SetViewRef, _, cx| {
                this.view_ref = a.0.clone();
                this.load_repo_name(cx);
                cx.notify();
                // The workspace saves the layout and redraws the tab bar.
                cx.propagate();
            }))
            .child(self.content.clone())
    }
}
