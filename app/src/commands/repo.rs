//! Owner: agent 2 (repository overview, repository lists, sidebar).

use fl_db::queries::repo;
use fl_types::{
    CmdError, CmdResult, Page, PageReq, RepoFilter, RepoId, RepoOverview, RepoRow, RepoSummary,
};
use tauri::{AppHandle, State};

use super::{emit_changed, read, write};
use crate::AppState;

#[tauri::command]
#[specta::specta]
pub async fn repos_pinned(state: State<'_, AppState>) -> CmdResult<Vec<RepoSummary>> {
    read(&state, repo::pinned).await
}

#[tauri::command]
#[specta::specta]
pub async fn repos_list(
    state: State<'_, AppState>,
    filter: RepoFilter,
    page: PageReq,
) -> CmdResult<Page<RepoRow>> {
    let page = page.clamped();
    read(&state, move |c| repo::list(c, &filter, page)).await
}

#[tauri::command]
#[specta::specta]
pub async fn repo_overview(state: State<'_, AppState>, repo_id: RepoId) -> CmdResult<RepoOverview> {
    read(&state, move |c| repo::overview(c, repo_id))
        .await?
        .ok_or(CmdError::NotFound)
}

/// Local-only write. Sync (agent 1) deep-syncs pinned repos.
#[tauri::command]
#[specta::specta]
pub async fn repo_set_pinned(
    app: AppHandle,
    state: State<'_, AppState>,
    repo_id: RepoId,
    pinned: bool,
) -> CmdResult<()> {
    if !write(&state, move |c| repo::set_pinned(c, repo_id, pinned)).await? {
        return Err(CmdError::NotFound);
    }
    if pinned {
        // Start the repo's deep sync now instead of on the next tick.
        state.sync.wake();
    }
    emit_changed(&app, &["repos"])
}
