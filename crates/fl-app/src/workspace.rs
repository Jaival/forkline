//! The main window: rail, dock (tabs + split panes), footer.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use fl_db::queries::{inbox, layout, repo, views};
use fl_types::{DataChanged, QueueKind, RepoSummary};
use gpui_kit::base::Placement;
use gpui_kit::component::dock::{
    ClosePanel, DockArea, DockAreaState, DockEvent, DockLayout, DockPlacement, DockSkin,
    InsertTarget, NodeId, PaneRef, PanelId, panel_handle,
};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{ActiveTheme as _, ThemeMode, h_flex, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, KeyBinding,
    FocusHandle, MouseButton, ParentElement as _, PromptLevel, Render, Styled as _,
    Subscription, Task, Window, actions,
    deferred, div, px,
};

use crate::appearance::{Appearance, AppearanceEvent};
use crate::drafts::{self, Drafts};
use crate::footer::{Footer, FooterEvent};
use crate::palette::{Palette, PaletteEvent, Run};
use crate::panel::ViewPanel;
use crate::rail;
use crate::signin::{SignIn, SignInEvent};
use crate::store::Store;
use crate::theme::ActivePalette as _;
use crate::view_ref::ViewRef;
use crate::theme::TextScale as _;

actions!(forkline, [NewTab, SplitRight, ToggleTheme, OpenPalette, OpenSignIn, OpenAppearance, NextTab, PrevTab]);

// A tab's view changed state the saved layout holds (e.g. a queue went flat).
actions!(forkline, [SaveLayout]);

/// Ask the workspace to show a view, so child views need no workspace handle.
#[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
#[action(namespace = forkline, no_json)]
pub struct OpenView(pub ViewRef);

/// Go to a kind of view: select an open tab of that kind (keeping its query
/// or repo filter), else open `0`. The rail and the palette's "Go to" use it.
#[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
#[action(namespace = forkline, no_json)]
pub struct GoTo(pub ViewRef);

/// The tab's view now shows `0` (e.g. the Search view ran a new query). The
/// tab records it, so its title follows and the saved layout reopens it.
#[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
#[action(namespace = forkline, no_json)]
pub struct SetViewRef(pub ViewRef);

/// Remove the tab of this `ViewPanel` entity, past its `closable` (the PR
/// tab sends it after its drafts warning).
#[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
#[action(namespace = forkline, no_json)]
pub struct RemoveTab(pub gpui_kit::EntityId);

/// Settings key of the saved dock layout. Not `ui_layout`: that row was the
/// removed Tauri app's layout.
const LAYOUT_KEY: &str = "gpui_layout";
/// Bump when the default layout changes in a way old saved layouts break.
const LAYOUT_VERSION: usize = 2;
const SAVE_DELAY: Duration = Duration::from_secs(2);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-t", NewTab, None),
        KeyBinding::new("ctrl-w", ClosePanel, None),
        KeyBinding::new("ctrl-\\", SplitRight, None),
        KeyBinding::new("ctrl-k", OpenPalette, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PrevTab, None),
    ]);
}

pub struct Workspace {
    dock: Entity<DockArea>,
    footer: Entity<Footer>,
    pinned: Vec<RepoSummary>,
    inbox_unread: u32,
    /// False in a bench run: it neither loads nor overwrites the user's layout.
    save_layout: bool,
    /// The command palette, while open, and the focus it came from.
    palette: Option<Entity<Palette>>,
    palette_return: Option<FocusHandle>,
    _palette_subscription: Option<Subscription>,
    /// The sign-in dialog, while open, and the focus it came from.
    sign_in: Option<Entity<SignIn>>,
    sign_in_return: Option<FocusHandle>,
    _sign_in_subscription: Option<Subscription>,
    /// The "Appearance…" dialog, while open, and the focus it came from.
    appearance: Option<Entity<Appearance>>,
    appearance_return: Option<FocusHandle>,
    _appearance_subscription: Option<Subscription>,
    /// `--open-menu`: menus shown at launch for screenshots.
    menu_preview: Vec<Entity<PopupMenu>>,
    _save_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(save_layout: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::bench::mark("workspace_new", cx);
        let (dock, _skin) = DockSkin::dock_area("main", Some(LAYOUT_VERSION), window, cx);
        let store = Store::global(cx);

        // Read synchronously: one small row, and it decides the first frame.
        // A bench run starts from the default layout.
        let saved = save_layout
            .then(|| store.read(cx).db().read(|c| layout::setting(c, LAYOUT_KEY)).ok().flatten())
            .flatten();
        let restored = saved
            .and_then(|json| serde_json::from_str::<DockAreaState>(&json).ok())
            .filter(|state| state.version == Some(LAYOUT_VERSION))
            .is_some_and(|state| dock.update(cx, |dock, cx| dock.load(state, window, cx).is_ok()));
        if !restored {
            let home = panel_handle(ViewPanel::new(ViewRef::Inbox, window, cx));
            let center = DockLayout::v_split().child(DockLayout::tabs().panel_view(home, cx), None);
            dock.update(cx, |dock, cx| dock.set_center(center, window, cx));
        }

        let footer = cx.new(Footer::new);
        let subscriptions = vec![
            cx.subscribe_in(&footer, window, |this, _, ev: &FooterEvent, window, cx| match ev {
                FooterEvent::SignIn => this.open_sign_in(window, cx),
            }),
            cx.observe_window_appearance(window, |this, window, cx| this.on_system_appearance(window, cx)),
            cx.subscribe_in(&dock, window, |this, _, ev: &DockEvent, window, cx| {
                if matches!(ev, DockEvent::LayoutChanged) {
                    this.schedule_save(window, cx);
                    cx.notify();
                }
            }),
            // A PR tab with drafts draws its own close button: redraw the tab bars.
            cx.observe(&Drafts::global(cx), |this, _, cx| this.dock.update(cx, |_, cx| cx.notify())),
            cx.subscribe(&store, |this, _, ev: &DataChanged, cx| {
                if ev.scopes.iter().any(|s| s == "repos" || s == "issues") {
                    this.load_pinned(cx);
                }
                if ev.scopes.iter().any(|s| s == "inbox") {
                    this.load_inbox_unread(cx);
                }
            }),
        ];

        let mut this = Self {
            dock,
            footer,
            pinned: Vec::new(),
            inbox_unread: 0,
            save_layout,
            palette: None,
            palette_return: None,
            _palette_subscription: None,
            sign_in: None,
            sign_in_return: None,
            _sign_in_subscription: None,
            appearance: None,
            appearance_return: None,
            _appearance_subscription: None,
            menu_preview: Vec::new(),
            _save_task: None,
            _subscriptions: subscriptions,
        };
        this.load_pinned(cx);
        this.load_inbox_unread(cx);
        warn_on_quit(window, cx);
        crate::bench::mark("workspace_built", cx);
        // Keys need a focused element; start on the active tab.
        cx.defer_in(window, |this, window, cx| {
            if let Some((_, id)) = this.focused_group(window, cx) {
                this.focus_panel(id, window, cx);
            }
        });
        this
    }

    /// Show `view_ref`: select its tab if one is open, else open it in the
    /// focused pane.
    pub fn open(&mut self, view_ref: ViewRef, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.find(&view_ref, cx) {
            self.dock.update(cx, |dock, cx| dock.select_panel(id, window, cx));
            self.focus_panel(id, window, cx);
            return;
        }
        let target = self.focused_group(window, cx);
        let panel = panel_handle(ViewPanel::new(view_ref, window, cx));
        let id = panel.panel_id(cx);
        self.dock.update(cx, |dock, cx| {
            dock.add_panel_view(panel, DockPlacement::Center, None, window, cx);
            if let Some((node, _)) = target {
                dock.move_panel(id, InsertTarget::Tabs { node, ix: None, activate: true }, window, cx);
            }
        });
        self.focus_panel(id, window, cx);
    }

    /// Select an open tab of `view_ref`'s kind, in the focused pane first,
    /// then in layout order; else open `view_ref`. A Search tab also gets its
    /// bar focused with the query selected, so typing replaces it.
    pub fn go_to(&mut self, view_ref: ViewRef, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.find_kind(&view_ref, window, cx) else {
            self.open(view_ref, window, cx);
            return;
        };
        self.dock.update(cx, |dock, cx| dock.select_panel(id, window, cx));
        self.focus_panel(id, window, cx);
        if let Some(search) = self.content_of(id, cx).and_then(|v| v.downcast::<crate::views::search::SearchView>().ok()) {
            search.update(cx, |search, cx| search.select_bar(window, cx));
        }
    }

    fn on_go_to(&mut self, action: &GoTo, window: &mut Window, cx: &mut Context<Self>) {
        self.go_to(action.0.clone(), window, cx);
    }

    fn on_next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_tab(1, window, cx);
    }

    fn on_prev_tab(&mut self, _: &PrevTab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_tab(-1, window, cx);
    }

    /// Select and focus the tab `step` away in the focused pane (wraps).
    fn cycle_tab(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((node_id, _)) = self.focused_group(window, cx) else { return };
        let mut next = None;
        if let Some(layout) = self.dock.read(cx).layout(DockPlacement::Center) {
            layout.root().walk(&mut |node| {
                if let PaneRef::Tabs { panels, active_ix } = node.kind()
                    && node.id() == node_id
                    && !panels.is_empty()
                {
                    let ix = (active_ix as isize + step).rem_euclid(panels.len() as isize) as usize;
                    next = Some(panels[ix]);
                }
            });
        }
        let Some(id) = next else { return };
        self.dock.update(cx, |dock, cx| dock.select_panel(id, window, cx));
        self.focus_panel(id, window, cx);
    }

    fn on_new_tab(&mut self, _: &NewTab, window: &mut Window, cx: &mut Context<Self>) {
        let panel = panel_handle(ViewPanel::new(ViewRef::Inbox, window, cx));
        let id = panel.panel_id(cx);
        let target = self.focused_group(window, cx);
        self.dock.update(cx, |dock, cx| {
            dock.add_panel_view(panel, DockPlacement::Center, None, window, cx);
            if let Some((node, _)) = target {
                dock.move_panel(id, InsertTarget::Tabs { node, ix: None, activate: true }, window, cx);
            }
        });
        self.focus_panel(id, window, cx);
    }

    /// Open the focused tab's view again in a new pane on the right.
    fn on_split_right(&mut self, _: &SplitRight, window: &mut Window, cx: &mut Context<Self>) {
        let Some((node, active)) = self.focused_group(window, cx) else { return };
        let Some(view_ref) = self.view_ref_of(active, cx) else { return };
        let panel = panel_handle(ViewPanel::new(view_ref, window, cx));
        let id = panel.panel_id(cx);
        self.dock.update(cx, |dock, cx| {
            dock.add_panel_view(panel, DockPlacement::Center, None, window, cx);
            dock.move_panel(id, InsertTarget::Split { node, placement: Placement::Right, size: None }, window, cx);
        });
        self.focus_panel(id, window, cx);
    }

    fn on_open_view(&mut self, action: &OpenView, window: &mut Window, cx: &mut Context<Self>) {
        self.open(action.0.clone(), window, cx);
    }

    /// The panel has taken the new `ViewRef` already; save it and redraw the tabs.
    fn on_set_view_ref(&mut self, _: &SetViewRef, window: &mut Window, cx: &mut Context<Self>) {
        self.schedule_save(window, cx);
        self.dock.update(cx, |_, cx| cx.notify());
        cx.notify();
    }

    fn on_save_layout(&mut self, _: &SaveLayout, window: &mut Window, cx: &mut Context<Self>) {
        self.schedule_save(window, cx);
    }

    fn on_remove_tab(&mut self, action: &RemoveTab, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.panel(PanelId::from(action.0), cx) else { return };
        self.dock.update(cx, |dock, cx| dock.remove_panel(panel, window, cx));
    }

    fn on_open_palette(&mut self, _: &OpenPalette, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.is_some() {
            self.close_palette(window, cx);
            return;
        }
        self.palette_return = window.focused(cx);
        let palette = cx.new(|cx| Palette::new(&self.pinned, window, cx));
        self._palette_subscription = Some(cx.subscribe_in(&palette, window, Self::on_palette_event));
        self.palette = Some(palette);
        cx.notify();
    }

    fn on_open_sign_in(&mut self, _: &OpenSignIn, window: &mut Window, cx: &mut Context<Self>) {
        self.open_sign_in(window, cx);
    }

    /// Show the sign-in dialog (once the footer has read the sign-in state).
    fn open_sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(auth) = self.footer.read(cx).auth().cloned() else { return };
        if self.sign_in.is_some() {
            return;
        }
        if self.palette.is_some() {
            self.close_palette(window, cx);
        }
        self.sign_in_return = window.focused(cx);
        let dialog = cx.new(|cx| SignIn::new(auth, window, cx));
        self._sign_in_subscription = Some(cx.subscribe_in(&dialog, window, |this, _, _: &SignInEvent, window, cx| {
            this.close_sign_in(window, cx);
        }));
        self.sign_in = Some(dialog);
        cx.notify();
    }

    fn on_open_appearance(&mut self, _: &OpenAppearance, window: &mut Window, cx: &mut Context<Self>) {
        self.open_appearance(window, cx);
    }

    /// Show the "Appearance…" dialog (from the rail's theme menu or Ctrl+K).
    /// Windows switched between dark and light app mode. In System mode,
    /// show the other mode's colours; GPUI also resets the title bar to the
    /// OS mode, so set it back to the palette in every mode. While the
    /// Appearance dialog is open it shows the mode being edited instead.
    fn on_system_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let system = cx.try_global::<crate::theme::Choices>().is_none_or(|c| c.mode.is_none());
        if system && self.appearance.is_none() {
            crate::theme::apply(None, Some(window), cx);
        } else {
            crate::theme::sync_title_bar(window, cx.palette().dark);
        }
        window.refresh();
    }

    /// Show `menus` near the top left (`--open-menu`, see `menu_preview.rs`).
    pub(crate) fn preview_menus(&mut self, menus: Vec<Entity<PopupMenu>>, cx: &mut Context<Self>) {
        self.menu_preview = menus;
        cx.notify();
    }

    pub(crate) fn open_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.appearance.is_some() || self.sign_in.is_some() {
            return;
        }
        if self.palette.is_some() {
            self.close_palette(window, cx);
        }
        self.appearance_return = window.focused(cx);
        let dialog = cx.new(|cx| Appearance::new(window, cx));
        self._appearance_subscription =
            Some(cx.subscribe_in(&dialog, window, |this, _, _: &AppearanceEvent, window, cx| {
                this.appearance = None;
                this._appearance_subscription = None;
                match this.appearance_return.take() {
                    Some(focus) => window.focus(&focus, cx),
                    None => {
                        if let Some((_, id)) = this.focused_group(window, cx) {
                            this.focus_panel(id, window, cx);
                        }
                    }
                }
                cx.notify();
            }));
        self.appearance = Some(dialog);
        cx.notify();
    }

    /// The open "Appearance…" dialog, if any.
    pub(crate) fn appearance(&self) -> Option<&Entity<Appearance>> {
        self.appearance.as_ref()
    }

    fn close_sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sign_in = None;
        self._sign_in_subscription = None;
        match self.sign_in_return.take() {
            Some(focus) => window.focus(&focus, cx),
            None => {
                if let Some((_, id)) = self.focused_group(window, cx) {
                    self.focus_panel(id, window, cx);
                }
            }
        }
        cx.notify();
    }

    /// Hide the palette and give focus back to where it came from.
    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        self._palette_subscription = None;
        match self.palette_return.take() {
            Some(focus) => window.focus(&focus, cx),
            None => {
                if let Some((_, id)) = self.focused_group(window, cx) {
                    self.focus_panel(id, window, cx);
                }
            }
        }
        cx.notify();
    }

    fn on_palette_event(&mut self, _: &Entity<Palette>, event: &PaletteEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.close_palette(window, cx);
        let PaletteEvent::Run(run) = event else { return };
        match run {
            // Dispatch is deferred, so it runs with focus back on the view.
            Run::Action(action) => window.dispatch_action(action.boxed_clone(), cx),
            Run::OpenSaved(view) => {
                let view_ref = match view.kind {
                    QueueKind::Inbox => ViewRef::Inbox,
                    QueueKind::Pulls => ViewRef::Pulls { repo_id: None },
                    QueueKind::Issues => ViewRef::Issues { repo_id: None },
                    QueueKind::Reviews => ViewRef::Reviews,
                };
                self.open(view_ref.clone(), window, cx);
                // By view, not by focus: a new tab has not rendered yet, so
                // the focus does not tell its group apart.
                let content = self.find(&view_ref, cx).and_then(|id| self.content_of(id, cx));
                if let Some(content) = content {
                    crate::views::set_filter(&content, &view.query, window, cx);
                }
            }
            Run::DeleteView(id) => {
                let (id, db) = (*id, Store::global(cx).read(cx).db().clone());
                let task = cx.background_spawn(async move { db.write(|c| views::delete(c, id)) });
                cx.spawn(async move |_, cx| {
                    if task.await.is_ok() {
                        cx.update(|cx| Store::emit_changed(&["views"], cx));
                    }
                })
                .detach();
            }
        }
    }

    fn on_toggle_theme(&mut self, _: &ToggleTheme, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_theme(window, cx);
    }

    pub(crate) fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if cx.theme().is_dark() { ThemeMode::Light } else { ThemeMode::Dark };
        self.set_mode(Some(mode), window, cx);
    }

    /// Show `mode` (`None`: follow the system) and save it as `theme`.
    pub(crate) fn set_mode(&mut self, mode: Option<ThemeMode>, window: &mut Window, cx: &mut Context<Self>) {
        let mut choices = cx.try_global::<crate::theme::Choices>().copied().unwrap_or_default();
        choices.mode = mode;
        cx.set_global(choices);
        crate::theme::apply(mode, Some(window), cx);
        window.refresh();
        let value = choices.settings().into_iter().find(|(key, _)| *key == "theme").and_then(|(_, v)| v);
        let db = Store::global(cx).read(cx).db().clone();
        cx.background_spawn(async move {
            let _ = db.write(|c| match &value {
                Some(v) => layout::set_setting(c, "theme", v),
                None => layout::delete_setting(c, "theme"),
            });
        })
        .detach();
    }

    pub(crate) fn toggle_account(&mut self, cx: &mut Context<Self>) {
        self.footer.update(cx, |footer, cx| footer.toggle_account(cx));
    }

    /// Tab group holding the focused tab (or the first group), and its active panel.
    fn focused_group(&self, window: &Window, cx: &App) -> Option<(NodeId, PanelId)> {
        let dock = self.dock.read(cx);
        let mut first = None;
        let mut focused = None;
        dock.layout(DockPlacement::Center)?.root().walk(&mut |node| {
            if let PaneRef::Tabs { panels, active_ix } = node.kind() {
                let Some(&active) = panels.get(active_ix) else { return };
                first.get_or_insert((node.id(), active));
                let has_focus = panels.iter().any(|id| {
                    dock.panel(*id).is_some_and(|p| p.focus_handle(cx).contains_focused(window, cx))
                });
                if has_focus {
                    focused = Some((node.id(), active));
                }
            }
        });
        focused.or(first)
    }

    /// Open tab showing `view_ref`, if any.
    fn find(&self, view_ref: &ViewRef, cx: &App) -> Option<PanelId> {
        let mut found = None;
        self.dock.read(cx).layout(DockPlacement::Center)?.root().walk(&mut |node| {
            if let PaneRef::Tabs { panels, .. } = node.kind() {
                for id in panels {
                    if found.is_none() && self.view_ref_of(*id, cx).as_ref() == Some(view_ref) {
                        found = Some(*id);
                    }
                }
            }
        });
        found
    }

    /// Open tab of `view_ref`'s kind: in the focused pane if it has one,
    /// else the first in layout order.
    fn find_kind(&self, view_ref: &ViewRef, window: &Window, cx: &App) -> Option<PanelId> {
        let focused = self.focused_group(window, cx).map(|(node, _)| node);
        let (mut first, mut in_focused) = (None, None);
        self.dock.read(cx).layout(DockPlacement::Center)?.root().walk(&mut |node| {
            if let PaneRef::Tabs { panels, .. } = node.kind() {
                for id in panels {
                    if self.view_ref_of(*id, cx).is_some_and(|v| v.same_kind(view_ref)) {
                        first.get_or_insert(*id);
                        if Some(node.id()) == focused {
                            in_focused.get_or_insert(*id);
                        }
                    }
                }
            }
        });
        in_focused.or(first)
    }

    /// The content view of the focused tab.
    #[cfg(test)]
    pub(crate) fn active_view(&self, window: &Window, cx: &App) -> Option<gpui_kit::AnyView> {
        let (_, id) = self.focused_group(window, cx)?;
        self.content_of(id, cx)
    }

    fn content_of(&self, id: PanelId, cx: &App) -> Option<gpui_kit::AnyView> {
        Some(self.panel(id, cx)?.read(cx).content().clone())
    }

    #[cfg(test)]
    pub(crate) fn footer(&self) -> &Entity<Footer> {
        &self.footer
    }

    /// The open sign-in dialog, if any.
    #[cfg(test)]
    pub(crate) fn sign_in(&self) -> Option<&Entity<SignIn>> {
        self.sign_in.as_ref()
    }

    /// The open command palette, if any.
    #[cfg(test)]
    pub(crate) fn palette(&self) -> Option<&Entity<Palette>> {
        self.palette.as_ref()
    }

    /// Tab groups in the center, each as its list of views (active one marked `*`).
    #[cfg(test)]
    pub(crate) fn tab_groups(&self, cx: &App) -> Vec<Vec<String>> {
        let mut out = Vec::new();
        self.dock.read(cx).layout(DockPlacement::Center).unwrap().root().walk(&mut |node| {
            if let PaneRef::Tabs { panels, active_ix } = node.kind() {
                out.push(
                    panels
                        .iter()
                        .enumerate()
                        .map(|(i, id)| {
                            let title = self.panel(*id, cx).unwrap().read(cx).title().to_string();
                            if i == active_ix { format!("*{title}") } else { title }
                        })
                        .collect(),
                );
            }
        });
        out
    }

    fn panel(&self, id: PanelId, cx: &App) -> Option<Entity<ViewPanel>> {
        self.dock.read(cx).panel(id)?.view().downcast::<ViewPanel>().ok()
    }

    fn view_ref_of(&self, id: PanelId, cx: &App) -> Option<ViewRef> {
        Some(self.panel(id, cx)?.read(cx).view_ref().clone())
    }

    fn focus_panel(&self, id: PanelId, window: &mut Window, cx: &mut App) {
        if let Some(panel) = self.dock.read(cx).panel(id) {
            window.focus(&panel.focus_handle(cx), cx);
        }
    }

    fn load_pinned(&mut self, cx: &mut Context<Self>) {
        let task = Store::global(cx).read(cx).read(cx, repo::pinned);
        cx.spawn(async move |this, cx| {
            if let Ok(pinned) = task.await {
                let _ = this.update(cx, |this, cx| {
                    this.pinned = pinned;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn load_inbox_unread(&mut self, cx: &mut Context<Self>) {
        let task = Store::global(cx).read(cx).read(cx, inbox::unread_count);
        cx.spawn(async move |this, cx| {
            if let Ok(count) = task.await {
                let _ = this.update(cx, |this, cx| {
                    this.inbox_unread = count;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Save the layout a moment after the last change (tab drags fire many).
    fn schedule_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.save_layout {
            return;
        }
        let dock = self.dock.clone();
        self._save_task = Some(cx.spawn_in(window, async move |_, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let Ok(json) = cx.update(|_, cx| serde_json::to_string(&dock.read(cx).dump(cx))) else {
                return;
            };
            let Ok(json) = json else { return };
            let Ok(db) = cx.update(|_, cx| Store::global(cx).read(cx).db().clone()) else { return };
            cx.background_executor()
                .spawn(async move {
                    let _ = db.write(|c| layout::set_setting(c, LAYOUT_KEY, &json));
                })
                .await;
        }));
    }
}

/// Closing the window with unsent inline drafts asks first: they live in
/// memory only. "Quit" closes the window (the app quits with its last one).
fn warn_on_quit(window: &mut Window, cx: &App) {
    let prompting = Rc::new(Cell::new(false));
    let confirmed = Rc::new(Cell::new(false));
    window.on_window_should_close(cx, move |window, cx| {
        let (prs, n) = Drafts::global(cx).read(cx).totals();
        if n == 0 || confirmed.get() {
            return true;
        }
        if !prompting.get() {
            prompting.set(true);
            let detail = format!("{}. Quitting drops them.", drafts::warning(prs, n));
            let answer =
                window.prompt(PromptLevel::Warning, "Quit Forkline?", Some(&detail), &["Quit", "Cancel"], cx);
            let (prompting, confirmed) = (prompting.clone(), confirmed.clone());
            window
                .spawn(cx, async move |cx| {
                    prompting.set(false);
                    if answer.await == Ok(0) {
                        confirmed.set(true);
                        let _ = cx.update(|window, _| window.remove_window());
                    }
                })
                .detach();
        }
        false
    });
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.focused_group(window, cx).and_then(|(_, id)| self.view_ref_of(id, cx));
        let login = self
            .footer
            .read(cx)
            .auth()
            .filter(|a| a.signed_in)
            .map(|a| a.login.clone().unwrap_or_else(|| "?".to_owned()));
        let rail = rail::render(&cx.entity(), active.as_ref(), self.inbox_unread, &self.pinned, login.as_deref(), cx);
        v_flex()
            .id("workspace")
            .key_context("Workspace")
            .on_action(cx.listener(Self::on_new_tab))
            .on_action(cx.listener(Self::on_split_right))
            .on_action(cx.listener(Self::on_toggle_theme))
            .on_action(cx.listener(Self::on_open_view))
            .on_action(cx.listener(Self::on_go_to))
            .on_action(cx.listener(Self::on_set_view_ref))
            .on_action(cx.listener(Self::on_save_layout))
            .on_action(cx.listener(Self::on_remove_tab))
            .on_action(cx.listener(Self::on_open_palette))
            .on_action(cx.listener(Self::on_open_sign_in))
            .on_action(cx.listener(Self::on_open_appearance))
            .on_action(cx.listener(Self::on_next_tab))
            .on_action(cx.listener(Self::on_prev_tab))
            .size_full()
            .bg(cx.palette().bg)
            .text_color(cx.palette().text)
            .text_body()
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(rail)
                    .child(div().flex_1().min_w_0().h_full().child(self.dock.clone())),
            )
            .child(self.footer.clone())
            .children(self.palette.clone().map(|palette| {
                // Over the dock; a click outside the box closes it.
                deferred(
                    div()
                        .id("palette-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| this.close_palette(window, cx)))
                        .child(
                            div().w_full().flex().justify_center().pt(px(80.)).child(
                                div()
                                    .id("palette-box")
                                    .occlude()
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .child(palette),
                            ),
                        ),
                )
                .with_priority(1)
            }))
            .children(self.sign_in.clone().map(|dialog| {
                // Modal: the backdrop takes clicks but does not close it.
                deferred(
                    div()
                        .id("signin-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .occlude()
                        .bg(cx.palette().backdrop)
                        .flex()
                        .justify_center()
                        .pt(px(120.))
                        .child(div().child(dialog)),
                )
                .with_priority(2)
            }))
            .children(self.appearance.clone().map(|dialog| {
                // Modal, but without a dimmed backdrop: the app behind it
                // shows each change live.
                deferred(
                    div()
                        .id("appearance-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .occlude()
                        .flex()
                        .justify_center()
                        .pt(px(48.))
                        .pb(px(24.))
                        .items_start()
                        .child(div().flex().max_h_full().child(dialog)),
                )
                .with_priority(2)
            }))
            .children((!self.menu_preview.is_empty()).then(|| {
                deferred(
                    h_flex()
                        .absolute()
                        .top(px(48.))
                        .left(px(64.))
                        .gap(px(12.))
                        .items_start()
                        .children(self.menu_preview.clone()),
                )
                .with_priority(3)
            }))
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{TestAppContext, WindowOptions};

    use super::*;

    use crate::test_support::open;

    fn groups(ws: &Workspace, cx: &App) -> Vec<Vec<String>> {
        ws.tab_groups(cx)
    }

    /// The test window's OS appearance is always light.
    #[gpui_kit::test]
    fn an_os_mode_change_shows_the_os_mode_only_in_system_mode(cx: &mut TestAppContext) {
        use crate::theme::Choices;
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.run_until_parked();
        let os_changed = |cx: &mut TestAppContext, mode: Option<ThemeMode>| {
            cx.update_window(handle, |_, window, cx| {
                cx.set_global(Choices { mode, ..Choices::default() });
                crate::theme::apply(Some(ThemeMode::Dark), Some(window), cx);
                ws.update(cx, |ws, cx| ws.on_system_appearance(window, cx));
            })
            .unwrap();
            cx.read(|cx| cx.palette().name)
        };
        assert_eq!(os_changed(cx, Some(ThemeMode::Dark)), "midnight");
        assert_eq!(os_changed(cx, None), "daylight");
    }

    #[gpui_kit::test]
    fn keys_open_tabs_and_split_panes(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.run_until_parked();
        cx.read(|cx| assert_eq!(groups(ws.read(cx), cx), [["*Inbox"]]));

        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.press("ctrl-t", cx);
        })
        .unwrap();
        cx.read(|cx| assert_eq!(groups(ws.read(cx), cx), [["Inbox", "*Inbox"]]));

        cx.update_window(handle, |_, window, cx| window.press("ctrl-\\", cx)).unwrap();
        cx.read(|cx| assert_eq!(groups(ws.read(cx), cx), [vec!["Inbox", "*Inbox"], vec!["*Inbox"]]));
    }

    #[gpui_kit::test]
    fn ctrl_tab_cycles_the_focused_pane(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open(ViewRef::Home, window, cx);
                ws.open(ViewRef::Repos, window, cx);
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.read(|cx| assert_eq!(groups(ws.read(cx), cx), [["Inbox", "Home", "*Repositories"]]));
        let press = |key: &str, cx: &mut TestAppContext| {
            cx.update_window(handle, |_, window, cx| {
                window.press(key, cx);
                window.render_frame(cx);
            })
            .unwrap();
            cx.run_until_parked();
        };
        // Wraps forward, then back.
        press("ctrl-tab", cx);
        cx.read(|cx| assert_eq!(groups(ws.read(cx), cx), [["*Inbox", "Home", "Repositories"]]));
        press("ctrl-shift-tab", cx);
        cx.read(|cx| assert_eq!(groups(ws.read(cx), cx), [["Inbox", "Home", "*Repositories"]]));
        press("ctrl-shift-tab", cx);
        cx.read(|cx| assert_eq!(groups(ws.read(cx), cx), [["Inbox", "*Home", "Repositories"]]));
        // The new tab has the focus, so the next key acts on its pane.
        let focused = cx
            .update_window(handle, |_, window, cx| ws.read(cx).focused_group(window, cx).map(|(_, id)| id))
            .unwrap();
        let home = cx.read(|cx| ws.read(cx).find(&ViewRef::Home, cx));
        assert_eq!(focused, home);
    }

    #[gpui_kit::test]
    fn open_selects_an_existing_tab(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            ws.update(cx, |ws, cx| {
                ws.open(ViewRef::Home, window, cx);
                ws.open(ViewRef::Inbox, window, cx);
                ws.open(ViewRef::Home, window, cx);
            });
        })
        .unwrap();
        cx.read(|cx| assert_eq!(groups(ws.read(cx), cx), [["Inbox", "*Home"]]));
    }

    #[gpui_kit::test]
    fn saved_layout_restores_tabs(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.run_until_parked();
        let json = cx
            .update_window(handle, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.open(ViewRef::Pr { id: 7 }, window, cx));
                serde_json::to_string(&ws.read(cx).dock.read(cx).dump(cx)).unwrap()
            })
            .unwrap();
        cx.update(|cx| {
            let db = Store::global(cx).read(cx).db().clone();
            db.write(|c| layout::set_setting(c, LAYOUT_KEY, &json)).unwrap();
            let options = WindowOptions::default();
            let (_, ws2) =
                gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Workspace::new(true, window, cx)))
                    .unwrap();
            assert_eq!(groups(ws2.read(cx), cx), [["Inbox", "*Pull request 7"]]);
        });
    }

    #[gpui_kit::test]
    fn a_flat_queue_tab_restores_flat(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.press("g", cx);
        })
        .unwrap();
        cx.run_until_parked();
        let json = cx
            .update_window(handle, |_, window, cx| {
                let view = ws.read(cx).active_view(window, cx).unwrap();
                assert_eq!(crate::views::flat_of(&view, cx), Some(true));
                serde_json::to_string(&ws.read(cx).dock.read(cx).dump(cx)).unwrap()
            })
            .unwrap();
        assert!(json.contains(r#""kind":"inbox","flat":true"#), "{json}");
        cx.update(|cx| {
            let db = Store::global(cx).read(cx).db().clone();
            db.write(|c| layout::set_setting(c, LAYOUT_KEY, &json)).unwrap();
            let options = WindowOptions::default();
            let (handle2, ws2) =
                gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Workspace::new(true, window, cx)))
                    .unwrap();
            handle2
                .update(cx, |_, window, cx| {
                    let view = ws2.read(cx).active_view(window, cx).unwrap();
                    assert_eq!(crate::views::flat_of(&view, cx), Some(true));
                })
                .unwrap();
        });
    }

    #[gpui_kit::test]
    fn rail_opens_views(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("rail-pulls", cx);
        })
        .unwrap();
        cx.read(|cx| assert_eq!(groups(ws.read(cx), cx), [["Inbox", "*Pull requests"]]));

        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("rail-inbox", cx);
        })
        .unwrap();
        cx.read(|cx| assert_eq!(groups(ws.read(cx), cx), [["*Inbox", "Pull requests"]]));
    }

    #[gpui_kit::test]
    fn open_view_action_opens_a_tab(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.dispatch_action(Box::new(OpenView(ViewRef::Pr { id: 7 })), cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.read(|cx| assert_eq!(groups(ws.read(cx), cx), [["Inbox", "*Pull request 7"]]));
    }

    #[gpui_kit::test]
    fn keys_reach_the_content_view(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = open(cx, dir.path());
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let (_, id) = ws.read(cx).focused_group(window, cx).unwrap();
            let focus = ws.read(cx).dock.read(cx).panel(id).unwrap().focus_handle(cx);
            assert!(focus.is_focused(window));
        })
        .unwrap();
    }

    #[gpui_kit::test]
    fn go_to_keeps_a_filtered_tab_and_prefers_the_focused_pane(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let (handle, ws) = crate::test_support::open_seeded(
            cx,
            dir.path(),
            "INSERT INTO repos(id, node_id, owner, name, updated_at) VALUES (1,'R','o','a',0), (2,'R','o','b',0);",
        );
        cx.run_until_parked();
        let open = |view_ref: ViewRef, cx: &mut TestAppContext| {
            cx.update_window(handle, |_, window, cx| {
                ws.update(cx, |ws, cx| ws.open(view_ref, window, cx));
                window.render_frame(cx);
            })
            .unwrap();
        };
        open(ViewRef::Pulls { repo_id: Some(1) }, cx);
        cx.update_window(handle, |_, window, cx| {
            window.press("ctrl-\\", cx);
            window.render_frame(cx);
        })
        .unwrap();
        open(ViewRef::Pulls { repo_id: Some(2) }, cx);
        cx.read(|cx| {
            assert_eq!(
                groups(ws.read(cx), cx),
                [vec!["Inbox", "*o/a: Pull requests"], vec!["o/a: Pull requests", "*o/b: Pull requests"]]
            )
        });

        // The right pane has focus: its first Pulls tab, not the left one.
        cx.update_window(handle, |_, window, cx| window.click("rail-pulls", cx)).unwrap();
        cx.read(|cx| {
            assert_eq!(
                groups(ws.read(cx), cx),
                [vec!["Inbox", "*o/a: Pull requests"], vec!["*o/a: Pull requests", "o/b: Pull requests"]]
            )
        });

        // No Issues tab open: it opens the unfiltered one.
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            window.click("rail-issues", cx);
        })
        .unwrap();
        cx.read(|cx| assert_eq!(groups(ws.read(cx), cx)[1], ["o/a: Pull requests", "o/b: Pull requests", "*Issues"]));
    }
}
