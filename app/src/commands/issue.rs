//! Owner: agent 3 (issue detail).

use fl_db::queries::issue;
use fl_types::{CmdError, CmdResult, IssueDetail, IssueId};
use tauri::State;

use super::{db_err, now_secs};
use crate::AppState;

/// Read an issue or PR from cache and clear its unread dot.
#[tauri::command]
#[specta::specta]
pub async fn issue_get(state: State<'_, AppState>, id: IssueId) -> CmdResult<IssueDetail> {
    let detail = state
        .db
        .read(|c| issue::get(c, id))
        .map_err(db_err)?
        .ok_or(CmdError::NotFound)?;
    if detail.row.unread {
        state
            .db
            .write(|c| issue::mark_viewed(c, id, now_secs()))
            .map_err(db_err)?;
    }
    Ok(detail)
}
