//! Owner: agent 4 (PR view).

use fl_db::queries::pr;
use fl_types::{CmdError, CmdResult, IssueId, Page, PageReq, PrDetail, ReviewThread};
use tauri::{AppHandle, State};

use super::{emit_changed, now_secs, read, write};
use crate::AppState;

/// Read a PR from cache and clear its unread dot.
#[tauri::command]
#[specta::specta]
pub async fn pr_get(
    app: AppHandle,
    state: State<'_, AppState>,
    id: IssueId,
) -> CmdResult<PrDetail> {
    let mut detail = read(&state, move |c| pr::get(c, id))
        .await?
        .ok_or(CmdError::NotFound)?;
    if detail.row.unread {
        write(&state, move |c| pr::mark_viewed(c, id, now_secs())).await?;
        detail.row.unread = false;
        emit_changed(&app, &["issues"])?;
    }
    Ok(detail)
}

#[tauri::command]
#[specta::specta]
pub async fn pr_threads(
    state: State<'_, AppState>,
    pr_id: IssueId,
    page: PageReq,
) -> CmdResult<Page<ReviewThread>> {
    let page = page.clamped();
    read(&state, move |c| pr::threads(c, pr_id, page)).await
}

/// Paths marked viewed at the PR's current head.
#[tauri::command]
#[specta::specta]
pub async fn pr_viewed_paths(state: State<'_, AppState>, pr_id: IssueId) -> CmdResult<Vec<String>> {
    read(&state, move |c| pr::viewed_paths(c, pr_id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn pr_set_viewed(
    app: AppHandle,
    state: State<'_, AppState>,
    pr_id: IssueId,
    path: String,
    viewed: bool,
) -> CmdResult<()> {
    write(&state, move |c| pr::set_viewed(c, pr_id, &path, viewed)).await?;
    emit_changed(&app, &["pr"])
}
