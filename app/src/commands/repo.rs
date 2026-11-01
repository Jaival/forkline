//! Owner: agent 2 (repository overview, sidebar).

use fl_db::queries::repo;
use fl_types::{CmdResult, RepoId, RepoSummary};
use tauri::State;

use super::{not_implemented, read};
use crate::AppState;

#[tauri::command]
#[specta::specta]
pub async fn repos_pinned(state: State<'_, AppState>) -> CmdResult<Vec<RepoSummary>> {
    read(&state, repo::pinned).await
}

#[tauri::command]
#[specta::specta]
pub async fn repo_overview(_repo_id: RepoId) -> CmdResult<()> {
    not_implemented("repo_overview")
}
