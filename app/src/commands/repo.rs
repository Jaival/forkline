//! Owner: agent 2 (repository overview, sidebar).

use fl_db::queries::repo;
use fl_types::{CmdResult, RepoId, RepoSummary};
use tauri::State;

use super::{db_err, not_implemented};
use crate::AppState;

#[tauri::command]
#[specta::specta]
pub async fn repos_pinned(state: State<'_, AppState>) -> CmdResult<Vec<RepoSummary>> {
    state.db.read(repo::pinned).map_err(db_err)
}

#[tauri::command]
#[specta::specta]
pub async fn repo_overview(_repo_id: RepoId) -> CmdResult<()> {
    not_implemented("repo_overview")
}
