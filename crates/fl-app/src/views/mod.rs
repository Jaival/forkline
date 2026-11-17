//! Tab content. Each G1 area adds its arm to [`build`] and its module here.

mod list_view;
mod placeholder;
mod queue;
mod reading;

pub use queue::SelectNext;

use fl_types::{QueueKind, QueueQuery};
use gpui_kit::{AnyView, App, AppContext as _, FocusHandle, KeyBinding, Window};

use crate::view_ref::ViewRef;

/// A built tab view and the focus handle of its root element. The panel
/// focuses this handle, so keys reach the view itself.
pub struct Built {
    pub view: AnyView,
    pub focus: FocusHandle,
}

/// Where the action keys of the reading pane work.
const TRIAGE: &str = "(Queue || Reading) && !Input";

/// Bind the keys of the views. Call once at startup.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("j", queue::SelectNext, Some("Queue")),
        KeyBinding::new("down", queue::SelectNext, Some("Queue")),
        KeyBinding::new("k", queue::SelectPrev, Some("Queue")),
        KeyBinding::new("up", queue::SelectPrev, Some("Queue")),
        KeyBinding::new("enter", queue::OpenSelected, Some("Queue")),
        // Not while typing: the reply box sits inside the reading pane.
        KeyBinding::new("a", reading::Approve, Some(TRIAGE)),
        KeyBinding::new("r", reading::Reply, Some(TRIAGE)),
        KeyBinding::new("e", reading::Done, Some(TRIAGE)),
        KeyBinding::new("h", reading::Snooze, Some(TRIAGE)),
        KeyBinding::new("escape", reading::CancelReply, Some("Reply")),
    ]);
}

/// Build the view for one tab.
pub fn build(view_ref: &ViewRef, window: &mut Window, cx: &mut App) -> Built {
    let mut list = |kind, repo_id| {
        list_view::ListView::build(QueueQuery { kind, repo_id }, view_ref.title(), window, cx)
    };
    match view_ref {
        ViewRef::Inbox => list(QueueKind::Inbox, None),
        ViewRef::Pulls { repo_id } => list(QueueKind::Pulls, *repo_id),
        ViewRef::Issues { repo_id } => list(QueueKind::Issues, *repo_id),
        ViewRef::Reviews => list(QueueKind::Reviews, None),
        ViewRef::Issue { id } => reading::ReadingPane::build(*id, false, cx),
        ViewRef::Pr { id } => reading::ReadingPane::build(*id, true, cx),
        _ => {
            let view = cx.new(|cx| placeholder::Placeholder::new(view_ref.clone(), cx));
            let focus = view.read(cx).focus.clone();
            Built { view: view.into(), focus }
        }
    }
}
