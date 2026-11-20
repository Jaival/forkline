//! Tab content. Each G1 area adds its arm to [`build`] and its module here.

mod explore;
mod home;
mod list_view;
mod placeholder;
pub(crate) mod queue;
mod reading;
mod rows;
mod repo;
mod repos;
pub(crate) mod search;

pub use queue::{FocusFilter, SaveView, SelectNext, ToggleGrouped};
pub use reading::{
    Approve, Done, EditAssignees, EditLabels, NextFile, PrevFile, Reply, ShowFiles, Snooze, ToggleState, ToggleViewed,
};

use fl_types::{QueueKind, QueueQuery};
use gpui_kit::{AnyView, App, AppContext as _, FocusHandle, KeyBinding, Window, actions};

use crate::theme::ActivePalette as _;
use crate::view_ref::ViewRef;

/// A built tab view and the focus handle of its root element. The panel
/// focuses this handle, so keys reach the view itself.
pub struct Built {
    pub view: AnyView,
    pub focus: FocusHandle,
}

/// Where the action keys of the reading pane work.
const TRIAGE: &str = "(Queue || Reading) && !Input";

/// Where the list keys work: the queue and the plain lists of step 7
/// (Repositories, Starred, Explore, Search, Home; key context `List`).
const LIST: &str = "(Queue || List) && !Input";

/// Where `/` works: a view with a query bar or a filter input.
const SLASH: &str = "(Queue || Reading || List) && !Input";

actions!(views, [TogglePin]);

/// The next selection in a flat list of `len` rows, `step` rows on. Nothing
/// is selected until the first move, which selects the first row.
pub(super) fn step(selected: Option<usize>, len: usize, step: isize) -> Option<usize> {
    let last = len.checked_sub(1)?;
    Some(match selected {
        None => 0,
        Some(ix) => ix.saturating_add_signed(step).min(last),
    })
}

/// Bind the keys of the views. Call once at startup.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        // Not while typing: the query bar sits inside the queue.
        KeyBinding::new("j", queue::SelectNext, Some(LIST)),
        KeyBinding::new("down", queue::SelectNext, Some(LIST)),
        KeyBinding::new("k", queue::SelectPrev, Some(LIST)),
        KeyBinding::new("up", queue::SelectPrev, Some(LIST)),
        KeyBinding::new("enter", queue::OpenSelected, Some(LIST)),
        KeyBinding::new("/", queue::FocusFilter, Some(SLASH)),
        // Repositories, Starred, Explore and a Repo tab: pin or unpin the repo.
        KeyBinding::new("p", TogglePin, Some("(List || Repo) && !Input")),
        // Not while typing: the reply box sits inside the reading pane.
        KeyBinding::new("a", reading::Approve, Some(TRIAGE)),
        KeyBinding::new("r", reading::Reply, Some(TRIAGE)),
        KeyBinding::new("x", reading::ToggleState, Some(TRIAGE)),
        KeyBinding::new("l", reading::EditLabels, Some(TRIAGE)),
        KeyBinding::new("e", reading::Done, Some(TRIAGE)),
        KeyBinding::new("h", reading::Snooze, Some(TRIAGE)),
        // Queue lists: grouped or flat. A plain key; never make `g` a prefix.
        KeyBinding::new("g", queue::ToggleGrouped, Some(TRIAGE)),
        // Files tab of a PR: act only while it is on show.
        KeyBinding::new("]", reading::NextFile, Some(TRIAGE)),
        KeyBinding::new("[", reading::PrevFile, Some(TRIAGE)),
        KeyBinding::new("v", reading::ToggleViewed, Some(TRIAGE)),
        KeyBinding::new("escape", reading::CancelReply, Some("Reply")),
        KeyBinding::new("escape", reading::ClosePicker, Some("Picker")),
    ]);
}

/// Set the `/` filter of `view` when it is a queue list (Inbox, Pulls,
/// Issues, Reviews): the query bar shows `query` and the queue applies it.
/// Returns false for any other view.
pub fn set_filter(view: &AnyView, query: &str, window: &mut Window, cx: &mut App) -> bool {
    let Ok(list) = view.clone().downcast::<list_view::ListView>() else { return false };
    let queue = list.read(cx).queue.clone();
    queue.update(cx, |queue, cx| queue.set_filter_text(query, window, cx));
    true
}

/// The `/` filter text of `view`, when it is a queue list.
#[cfg(test)]
pub fn filter_of(view: &AnyView, cx: &App) -> Option<String> {
    let list = view.clone().downcast::<list_view::ListView>().ok()?;
    Some(list.read(cx).queue.read(cx).filter().to_owned())
}

/// Whether `view` is a queue list shown flat; `None` for any other view.
pub fn flat_of(view: &AnyView, cx: &App) -> Option<bool> {
    let list = view.clone().downcast::<list_view::ListView>().ok()?;
    Some(list.read(cx).queue.read(cx).flat())
}

/// Build the view for one tab. `flat` applies to queue lists only.
pub fn build(view_ref: &ViewRef, flat: bool, window: &mut Window, cx: &mut App) -> Built {
    let mut list = |kind, repo_id| {
        list_view::ListView::build(QueueQuery { kind, repo_id, flat }, view_ref.title(), window, cx)
    };
    match view_ref {
        ViewRef::Inbox => list(QueueKind::Inbox, None),
        ViewRef::Pulls { repo_id } => list(QueueKind::Pulls, *repo_id),
        ViewRef::Issues { repo_id } => list(QueueKind::Issues, *repo_id),
        ViewRef::Reviews => list(QueueKind::Reviews, None),
        ViewRef::Issue { id } => reading::ReadingPane::build(*id, false, cx),
        ViewRef::Pr { id } => reading::ReadingPane::build(*id, true, cx),
        ViewRef::Repos => repos::ReposView::build(false, window, cx),
        ViewRef::Starred => repos::ReposView::build(true, window, cx),
        ViewRef::Explore => explore::ExploreView::build(window, cx),
        ViewRef::Repo { repo_id } => repo::RepoView::build(*repo_id, window, cx),
        ViewRef::Search { q } => search::SearchView::build(q, window, cx),
        ViewRef::Home => home::HomeView::build(window, cx),
        ViewRef::Settings => {
            let view = cx.new(|cx| placeholder::Placeholder::new(view_ref.clone(), cx));
            let focus = view.read(cx).focus.clone();
            Built { view: view.into(), focus }
        }
    }
}

/// A centred muted line for an empty list.
fn empty<V>(text: &'static str, cx: &gpui_kit::Context<V>) -> gpui_kit::AnyElement {
    use gpui_kit::{IntoElement as _, ParentElement as _, Styled as _};
    gpui_kit::div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_color(cx.palette().text_muted)
        .child(text)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::step;

    #[test]
    fn step_starts_at_the_first_row_and_stays_in_range() {
        assert_eq!(step(None, 0, 1), None);
        assert_eq!(step(None, 3, 1), Some(0));
        assert_eq!(step(None, 3, -1), Some(0));
        assert_eq!(step(Some(0), 3, -1), Some(0));
        assert_eq!(step(Some(1), 3, 1), Some(2));
        assert_eq!(step(Some(2), 3, 1), Some(2));
        assert_eq!(step(Some(5), 3, 1), Some(2));
    }
}
