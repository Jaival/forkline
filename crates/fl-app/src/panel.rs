//! One dock tab. Holds a [`ViewRef`] and the view built for it; the `ViewRef`
//! is what the saved layout records, so a restored tab rebuilds the same view.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{
    BasePanel, ClosePanel, Panel, PanelEvent, PanelInfo, PanelState, panel_handle, register_panel,
};
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use fl_db::queries::{issue, repo};
use fl_types::DataChanged;
use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, SharedString, Styled as _, Subscription,
    PromptLevel, Window, div, px,
};

use crate::drafts::{self, Drafts};
use crate::store::Store;
use crate::theme::ActivePalette as _;
use crate::view_ref::ViewRef;
use crate::views;
use crate::workspace::{RemoveTab, SetViewRef};

pub const PANEL_NAME: &str = "forkline.view";
/// Saved-layout key of a queue tab shown flat.
const FLAT_KEY: &str = "flat";

pub struct ViewPanel {
    view_ref: ViewRef,
    /// `owner/name` of the tab's repo, if it has one and the cache knows it.
    /// The `ViewRef` holds only the id, so the title looks the name up.
    repo_name: Option<String>,
    /// An Issue / PR tab's item: number, title, state, `owner/name`. Read
    /// like `repo_name`.
    item: Option<(i64, String, String, String)>,
    content: AnyView,
    focus_handle: FocusHandle,
    /// The close warning is open (one prompt at a time).
    prompting: bool,
    _subscription: Subscription,
    /// A PR tab: its drafts decide how the tab closes.
    _drafts: Option<Subscription>,
}

impl ViewPanel {
    pub fn new(view_ref: ViewRef, window: &mut Window, cx: &mut App) -> Entity<Self> {
        Self::build(view_ref, false, window, cx)
    }

    /// `flat`: a queue tab saved as flat (see [`views::flat_of`]).
    fn build(view_ref: ViewRef, flat: bool, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let built = views::build(&view_ref, flat, window, cx);
        cx.new(|cx| {
            let subscription = cx.subscribe(&Store::global(cx), |this: &mut Self, _, ev: &DataChanged, cx| {
                let item = matches!(this.view_ref, ViewRef::Pr { .. } | ViewRef::Issue { .. });
                let repos = this.view_ref.repo_id().is_some() && ev.scopes.iter().any(|s| s == "repos");
                if repos || (item && ev.scopes.iter().any(|s| s == "issues" || s == "pr")) {
                    this.load_repo_name(cx);
                    cx.notify();
                }
            });
            let drafts = matches!(view_ref, ViewRef::Pr { .. })
                .then(|| cx.observe(&Drafts::global(cx), |_, _, cx| cx.notify()));
            let mut this = Self {
                content: built.view,
                view_ref,
                repo_name: None,
                item: None,
                focus_handle: built.focus,
                prompting: false,
                _subscription: subscription,
                _drafts: drafts,
            };
            this.load_repo_name(cx);
            this
        })
    }

    /// Read synchronously: one row by key, and the tab title shows it on the
    /// first frame.
    fn load_repo_name(&mut self, cx: &App) {
        let db = Store::global(cx).read(cx).db();
        self.repo_name = self.view_ref.repo_id().and_then(|id| db.read(|c| repo::full_name(c, id)).ok().flatten());
        self.item = match self.view_ref {
            ViewRef::Pr { id } | ViewRef::Issue { id } => db.read(|c| issue::tab_label(c, id)).ok().flatten(),
            _ => None,
        };
    }

    /// `#42 Split panes` for an item, else the view's name.
    pub fn title(&self) -> SharedString {
        match &self.item {
            Some((number, title, ..)) => format!("#{number} {title}").into(),
            None => self.view_ref.title_with(self.repo_name.as_deref()),
        }
    }

    pub(crate) fn content(&self) -> &AnyView {
        &self.content
    }

    pub fn view_ref(&self) -> &ViewRef {
        &self.view_ref
    }

    /// Inline drafts of the PR this tab shows. While there are some the dock
    /// may not close the tab: the tab's own close button and `Ctrl+W` warn
    /// first. The drafts stay in memory either way.
    fn unsent_drafts(&self, cx: &App) -> usize {
        match self.view_ref {
            ViewRef::Pr { id } => Drafts::global(cx).read(cx).count(id),
            _ => 0,
        }
    }

    /// Ask before closing a PR tab with drafts; close it on "Close tab".
    fn warn_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.unsent_drafts(cx);
        if n == 0 || self.prompting {
            return;
        }
        self.prompting = true;
        let detail = format!(
            "{}. They stay until you quit Forkline; open the pull request again to send them.",
            drafts::warning(1, n)
        );
        let answer = window.prompt(PromptLevel::Warning, "Close this tab?", Some(&detail), &["Close tab", "Cancel"], cx);
        let id = cx.entity_id();
        cx.spawn_in(window, async move |this, cx| {
            let close = answer.await == Ok(0);
            let _ = this.update_in(cx, |this, window, cx| {
                this.prompting = false;
                if close {
                    window.dispatch_action(Box::new(RemoveTab(id)), cx);
                }
            });
        })
        .detach();
    }
}

/// Lets the dock rebuild saved tabs. Call once at startup.
pub fn init(cx: &mut App) {
    register_panel(cx, PANEL_NAME, |ctx, window, cx| {
        let (view_ref, flat) = match &ctx.state().info {
            PanelInfo::Panel(json) => (
                serde_json::from_value(json.clone()).unwrap_or(ViewRef::Home),
                json.get(FLAT_KEY).and_then(|v| v.as_bool()).unwrap_or(false),
            ),
            _ => (ViewRef::Home, false),
        };
        panel_handle(ViewPanel::build(view_ref, flat, window, cx))
    });
}

impl BasePanel for ViewPanel {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }

    /// False while the PR tab has drafts: the dock neither draws its X nor
    /// closes it on `Ctrl+W`; the tab warns first (`warn_close`).
    fn closable(&self, cx: &App) -> bool {
        self.unsent_drafts(cx) == 0
    }

    fn dump(&self, cx: &App) -> PanelState {
        let mut state = PanelState::new(PANEL_NAME);
        let mut json = serde_json::to_value(&self.view_ref).unwrap_or_default();
        // Beside the `ViewRef` fields: a view option, not what the tab shows.
        // `ViewRef` ignores the extra key when the tab is read back.
        if views::flat_of(&self.content, cx) == Some(true)
            && let Some(map) = json.as_object_mut()
        {
            map.insert(FLAT_KEY.into(), true.into());
        }
        state.info = PanelInfo::panel(json);
        state
    }
}

impl Panel for ViewPanel {
    /// Always `None`: the dock then draws [`Self::title`], with the view's
    /// icon (and the drafts warning close button).
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        None
    }

    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let drafts = self.unsent_drafts(cx) > 0;
        let p = cx.palette();
        let (icon, color) = match (&self.view_ref, self.item.as_ref().map(|(_, _, s, _)| s.as_str())) {
            (ViewRef::Pr { .. }, Some("merged")) => (IconName::GitMerge, p.merged.fg),
            (ViewRef::Pr { .. }, Some("closed")) => (IconName::GitPullRequestClosed, p.closed.fg),
            (ViewRef::Issue { .. }, Some("closed" | "merged")) => (IconName::CircleCheck, p.merged.fg),
            (ViewRef::Pr { .. } | ViewRef::Issue { .. }, Some(_)) => (self.view_ref.icon(), p.open.fg),
            _ => (self.view_ref.icon(), p.accent_text),
        };
        h_flex()
            .gap(px(8.))
            .text_size(crate::theme::TEXT_SECONDARY)
            .child(Icon::new(icon).with_size(px(13.)).text_color(color))
            .when_some(self.item.as_ref(), |d, (.., repo)| d.child(crate::repo_color::dot(repo, cx)))
            .child(div().max_w(px(220.)).truncate().child(ViewPanel::title(self)))
            .when(drafts, |d| {
                d.child(
                    Button::new("tab-close-drafts")
                        .icon(IconName::Close)
                        .xsmall()
                        .ghost()
                        .tooltip("Close (inline comments not sent)")
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.warn_close(window, cx);
                        })),
                )
            })
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
            .on_action(cx.listener(|this, _: &ClosePanel, window, cx| {
                if this.unsent_drafts(cx) > 0 {
                    this.warn_close(window, cx);
                } else {
                    cx.propagate();
                }
            }))
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
