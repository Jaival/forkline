//! Owner: agent 3 (issue detail).

use fl_db::queries::issue;
use fl_types::{CmdError, CmdResult, EditOptions, IssueDetail, IssueId, IssueThread, RepoId};
use tauri::{AppHandle, State};

use super::{emit_changed, now_secs, read, write};
use crate::AppState;

/// Read an issue or PR from cache and clear its unread dot.
#[tauri::command]
#[specta::specta]
pub async fn issue_get(
    app: AppHandle,
    state: State<'_, AppState>,
    id: IssueId,
) -> CmdResult<IssueDetail> {
    let mut detail = read(&state, move |c| issue::get(c, id))
        .await?
        .ok_or(CmdError::NotFound)?;
    if detail.row.unread {
        write(&state, move |c| issue::mark_viewed(c, id, now_secs())).await?;
        detail.row.unread = false;
        emit_changed(&app, &["issues"])?;
    }
    Ok(detail)
}

/// Conversation, timeline, assignees, linked items and pending fields.
#[tauri::command]
#[specta::specta]
pub async fn issue_thread(state: State<'_, AppState>, id: IssueId) -> CmdResult<IssueThread> {
    read(&state, move |c| issue::thread(c, id)).await
}

/// Labels and users to choose from in the editors.
#[tauri::command]
#[specta::specta]
pub async fn issue_edit_options(
    state: State<'_, AppState>,
    repo_id: RepoId,
) -> CmdResult<EditOptions> {
    read(&state, move |c| issue::edit_options(c, repo_id)).await
}
