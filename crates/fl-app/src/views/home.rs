//! Home ("Needs me now") is a queue (`QueueKind::Home`, see
//! `fl_db::queries::queue`) beside a reading pane, built by `list_view`.
//! This module keeps the issue-row helpers that Search shares, and the
//! one-time note after auto-pin.

use fl_db::queries::layout;
use fl_types::{IssueRow, IssueState};
use gpui_kit::assets::IconName;
use gpui_kit::Hsla;

use crate::view_ref::ViewRef;

/// Settings key: how many repos the first sync pinned. Home shows a note
/// while it is set; closing the note deletes it.
pub const AUTO_PIN_NOTE: &str = "auto_pin_note";

/// The note's text for `count` pinned repos.
pub fn auto_pin_text(count: u32) -> String {
    let repos = if count == 1 { "1 repo" } else { &format!("{count} repos") };
    format!(
        "Pinned {repos} you pushed to in the last {} days. Pinned repos sync all their issues and PRs.",
        fl_sync::sync::AUTO_PIN_DAYS
    )
}

/// The pending note's repo count, if any.
pub fn read_auto_pin_note(conn: &rusqlite::Connection) -> rusqlite::Result<Option<u32>> {
    Ok(layout::setting(conn, AUTO_PIN_NOTE)?.and_then(|v| v.trim().parse().ok()))
}

/// State icon and colour of an issue or PR.
pub(super) fn state_icon(row: &IssueRow, theme: &crate::theme::Palette) -> (IconName, Hsla) {
    match (row.is_pr, row.state) {
        (true, IssueState::Open) => (IconName::GitPullRequest, theme.open.fg),
        (true, IssueState::Merged) => (IconName::GitMerge, theme.merged.fg),
        (true, IssueState::Closed) => (IconName::GitPullRequestClosed, theme.closed.fg),
        (false, IssueState::Open) => (IconName::CircleDot, theme.open.fg),
        (false, _) => (IconName::CircleCheck, theme.merged.fg),
    }
}

/// The tab an issue row opens.
pub(super) fn item_ref(row: &IssueRow) -> ViewRef {
    if row.is_pr { ViewRef::Pr { id: row.id } } else { ViewRef::Issue { id: row.id } }
}

#[cfg(test)]
mod tests;
