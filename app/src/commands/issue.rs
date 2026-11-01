//! Owner: agent 3 (issue detail).

use fl_db::queries::issue;
use fl_types::{CmdError, CmdResult, IssueDetail, IssueId};
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
