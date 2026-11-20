//! The `Ctrl+K` command palette: go to a view, run an action of the item on
//! show, open a saved view, search issues and pull requests.
//!
//! The palette is an entity the workspace shows as an overlay. It never runs a
//! command itself: it emits [`PaletteEvent`] and the workspace closes it,
//! restores focus, then runs the command, so actions reach the view that had
//! focus before the palette opened.

use std::rc::Rc;

use fl_db::queries::views;
use fl_types::{PageReq, QueueKind, RepoSummary, SavedView};
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::dock::ClosePanel;
use gpui_kit::component::{IndexPath, h_flex, v_flex};
use gpui_kit::{
    Action, AnyElement, App, AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _,
    IntoElement, ParentElement as _, Render, SharedString, Styled as _, Window, div, px,
};

use crate::theme::ActivePalette as _;
use crate::store::Store;
use crate::view_ref::ViewRef;
use crate::views::{
    Approve, Done, EditAssignees, EditLabels, FocusFilter, NextFile, PrevFile, Reply, SaveView, ShowFiles, Snooze, ToggleGrouped, ToggleState, ToggleViewed,
};
use crate::workspace::{GoTo, NewTab, NextTab, OpenAppearance, OpenSignIn, OpenView, PrevTab, SplitRight, ToggleTheme};
use crate::theme::TextScale as _;

#[cfg(test)]
mod tests;

/// Most issue and pull request hits the palette lists.
const HIT_LIMIT: u32 = 8;

/// What the workspace does when a command is confirmed.
#[derive(Clone)]
pub enum Run {
    /// Dispatch to the view that had focus.
    Action(Rc<dyn Action>),
    /// Open the list of a saved view and apply its filter.
    OpenSaved(SavedView),
    DeleteView(i64),
}

pub enum PaletteEvent {
    Dismiss,
    Run(Run),
}

impl EventEmitter<PaletteEvent> for Palette {}

#[derive(Clone)]
enum Hint {
    Key(&'static str),
    Text(String),
}

#[derive(Clone)]
struct Cmd {
    label: String,
    hint: Option<Hint>,
    run: Run,
}

fn action(label: &str, hint: Option<&'static str>, action: impl Action) -> Cmd {
    Cmd { label: label.into(), hint: hint.map(Hint::Key), run: Run::Action(Rc::new(action)) }
}

fn open(label: String, view: ViewRef) -> Cmd {
    Cmd { label, hint: None, run: Run::Action(Rc::new(OpenView(view))) }
}

/// Selects an open tab of the view's kind (see `Workspace::go_to`).
fn go_to(label: &str, view: ViewRef) -> Cmd {
    Cmd { label: format!("Go to: {label}"), hint: None, run: Run::Action(Rc::new(GoTo(view))) }
}

pub struct Palette {
    state: Entity<CommandState>,
    /// Commands in display order, by group heading. Fixed while open.
    sections: Vec<(&'static str, Vec<Cmd>)>,
    /// Search hits for the current query.
    hits: Vec<Cmd>,
    /// Bumped per query, so a slow search never replaces a newer one.
    generation: u64,
    /// The groups as last rendered; a confirm index points into this.
    shown: Vec<Vec<Cmd>>,
}

impl Palette {
    /// `window` still has focus on the view that was active: the item actions
    /// listed here are the ones that view can run now.
    pub fn new(pinned: &[RepoSummary], window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.new(|cx| CommandState::new(window, cx));
        let mut go = vec![
            go_to("Inbox", ViewRef::Inbox),
            go_to("Pull requests", ViewRef::Pulls { repo_id: None }),
            go_to("Issues", ViewRef::Issues { repo_id: None }),
            go_to("Review requests", ViewRef::Reviews),
            go_to("Repositories", ViewRef::Repos),
            go_to("Starred", ViewRef::Starred),
            go_to("Explore", ViewRef::Explore),
            go_to("Search", ViewRef::Search { q: String::new() }),
            go_to("Settings", ViewRef::Settings),
        ];
        for repo in pinned {
            let name = format!("{}/{}", repo.owner, repo.name);
            // This repo's tab exactly, not any Pulls / Issues tab.
            go.push(open(format!("Go to: {name}: Pull requests"), ViewRef::Pulls { repo_id: Some(repo.id) }));
            go.push(open(format!("Go to: {name}: Issues"), ViewRef::Issues { repo_id: Some(repo.id) }));
        }

        // One small read, on open: the palette lives only while the user types.
        let saved = Store::global(cx).read(cx).db().read(views::list).unwrap_or_default();
        let mut saved_cmds = Vec::new();
        for view in &saved {
            let kind = match view.kind {
                QueueKind::Inbox => "Inbox",
                QueueKind::Pulls => "Pull requests",
                QueueKind::Issues => "Issues",
                QueueKind::Reviews => "Review requests",
            };
            saved_cmds.push(Cmd {
                label: format!("View: {}", view.name),
                hint: Some(Hint::Text(kind.into())),
                run: Run::OpenSaved(view.clone()),
            });
        }
        for view in &saved {
            saved_cmds.push(Cmd {
                label: format!("Delete view: {}", view.name),
                hint: None,
                run: Run::DeleteView(view.id),
            });
        }

        let items: Vec<Cmd> = [
            action("Approve", Some("A"), Approve),
            action("Reply", Some("R"), Reply),
            action("Close or reopen", Some("X"), ToggleState),
            action("Edit labels", Some("L"), EditLabels),
            action("Edit assignees", None, EditAssignees),
            action("Mark done", Some("E"), Done),
            action("Snooze for a day", Some("H"), Snooze),
            action("Show files", None, ShowFiles),
            action("Next file", Some("]"), NextFile),
            action("Previous file", Some("["), PrevFile),
            action("Toggle viewed", Some("V"), ToggleViewed),
            action("Filter queue", Some("/"), FocusFilter),
            action("Save filter as view", None, SaveView),
            action("Grouped or flat", Some("G"), ToggleGrouped),
        ]
        .into_iter()
        .filter(|cmd| match &cmd.run {
            Run::Action(a) => window.is_action_available(a.as_ref(), cx),
            _ => false,
        })
        .collect();

        let workspace = vec![
            action("New tab", Some("Ctrl+T"), NewTab),
            action("Close tab", Some("Ctrl+W"), ClosePanel),
            action("Split right", Some("Ctrl+\\"), SplitRight),
            action("Next tab", Some("Ctrl+Tab"), NextTab),
            action("Previous tab", Some("Ctrl+Shift+Tab"), PrevTab),
            action("Toggle theme", None, ToggleTheme),
            action("Appearance…", None, OpenAppearance),
            action("Sign in to GitHub", None, OpenSignIn),
        ];

        // After the availability check above: that one needs the old focus.
        state.update(cx, |state, cx| state.focus(window, cx));

        Self {
            state,
            sections: vec![("Actions", items), ("Go to", go), ("Saved views", saved_cmds), ("Workspace", workspace)],
            hits: Vec::new(),
            generation: 0,
            shown: Vec::new(),
        }
    }

    /// The groups for the current query: commands whose label holds every
    /// word, then search hits, then a search-all row.
    fn visible(&self, cx: &App) -> Vec<(&'static str, Vec<Cmd>)> {
        let query = self.state.read(cx).query(cx);
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        let mut out = Vec::new();
        for (heading, cmds) in &self.sections {
            let found: Vec<Cmd> = cmds
                .iter()
                .filter(|cmd| {
                    let label = cmd.label.to_lowercase();
                    words.iter().all(|word| label.contains(word))
                })
                .cloned()
                .collect();
            if !found.is_empty() {
                out.push((*heading, found));
            }
        }
        if !words.is_empty() {
            if !self.hits.is_empty() {
                out.push(("Issues and pull requests", self.hits.clone()));
            }
            let q = query.trim().to_owned();
            out.push(("Search", vec![open(format!("Search for \"{q}\""), ViewRef::Search { q })]));
        }
        out
    }

    /// Labels of the commands on show, in order.
    #[cfg(test)]
    pub(crate) fn labels(&self, cx: &App) -> Vec<String> {
        self.visible(cx).into_iter().flat_map(|(_, cmds)| cmds).map(|cmd| cmd.label).collect()
    }

    /// Look the query up in the search index; only the newest query's answer is kept.
    fn search(&mut self, query: &str, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let query = query.trim().to_owned();
        if query.is_empty() {
            self.hits.clear();
            cx.notify();
            return;
        }
        let task = Store::global(cx)
            .read(cx)
            .read(cx, move |c| fl_search::search(c, &query, PageReq { offset: 0, limit: HIT_LIMIT }));
        cx.spawn(async move |this, cx| {
            let Ok(page) = task.await else { return };
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.hits = page
                    .items
                    .into_iter()
                    .map(|hit| {
                        let row = hit.row;
                        let view = if row.is_pr { ViewRef::Pr { id: row.id } } else { ViewRef::Issue { id: row.id } };
                        Cmd {
                            label: format!("#{} {}", row.number, row.title),
                            hint: Some(Hint::Text(row.repo)),
                            run: Run::Action(Rc::new(OpenView(view))),
                        }
                    })
                    .collect();
                cx.notify();
            });
        })
        .detach();
    }

    fn confirm(&mut self, index: IndexPath, cx: &mut Context<Self>) {
        let Some(cmd) = self.shown.get(index.section).and_then(|group| group.get(index.row)) else { return };
        cx.emit(PaletteEvent::Run(cmd.run.clone()));
    }
}

/// A small key box, like the keys on the action bar.
fn kbd(text: impl Into<SharedString>, cx: &App) -> AnyElement {
    let theme = *cx.palette();
    div()
        .px(px(5.))
        .py(px(1.))
        .rounded(px(3.))
        .border_1()
        .border_color(theme.border)
        .text_small()
        .text_color(theme.text_muted)
        .child(text.into())
        .into_any_element()
}

impl Render for Palette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let visible = self.visible(cx);
        self.shown = visible.iter().map(|(_, cmds)| cmds.clone()).collect();

        let mut command = Command::new(&self.state)
            .filterable(false)
            .bordered(false)
            .placeholder("Type a command or search")
            .max_h(px(360.));
        for (heading, cmds) in visible {
            let items = cmds.into_iter().map(|cmd| {
                let label = cmd.label.clone();
                let hint = cmd.hint.clone();
                CommandItem::new().label(cmd.label).child(move |_, cx| {
                    let hint = hint.clone().map(|hint| match hint {
                        Hint::Key(key) => kbd(key, cx),
                        Hint::Text(text) => {
                            div().text_small().text_color(cx.palette().text_muted).child(text).into_any_element()
                        }
                    });
                    h_flex()
                        .w_full()
                        .gap(px(12.))
                        .items_center()
                        .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                        .children(hint)
                })
            });
            command = command.group(CommandGroup::new().label(heading).items(items));
        }
        let palette = cx.entity().downgrade();
        let (on_confirm, on_query) = (palette.clone(), palette.clone());
        let command = command
            .on_query(move |query, _, cx| {
                let _ = on_query.update(cx, |this, cx| this.search(query, cx));
            })
            .on_confirm(move |index, _, cx| {
                let _ = on_confirm.update(cx, |this, cx| this.confirm(index, cx));
            })
            .on_cancel(move |_, cx| {
                let _ = palette.update(cx, |_, cx| cx.emit(PaletteEvent::Dismiss));
            });

        let theme = *cx.palette();
        v_flex()
            .id("palette")
            .w(px(560.))
            .overflow_hidden()
            .rounded(px(10.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.control)
            .text_color(theme.text)
            .shadow_lg()
            .child(command)
            .child(
                h_flex()
                    .px(px(12.))
                    .py(px(6.))
                    .gap(px(8.))
                    .border_t_1()
                    .border_color(theme.border)
                    .text_small()
                    .text_color(theme.text_muted)
                    .child(div().flex_1().child("Enter to run"))
                    .child(kbd("Esc", cx)),
            )
    }
}
