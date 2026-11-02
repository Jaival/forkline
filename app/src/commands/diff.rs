//! Owner: agent 4 (diff viewer; reads diffs from fl-git, built by agent 5).

use fl_db::queries::pr::{self, DiffTarget};
use fl_types::{CmdError, CmdResult, DiffFile, DiffSummary, IssueId};
use tauri::State;

use super::read;
use crate::AppState;

/// Look up the PR's repo and oids, make sure the commits are local, then run
/// `f` on the clone. All git work runs on the blocking pool.
async fn with_clone<T: Send + 'static>(
    state: &AppState,
    pr_id: IssueId,
    f: impl FnOnce(&fl_git::GitStore, &std::path::Path, &DiffTarget) -> fl_git::GitResult<T>
    + Send
    + 'static,
) -> CmdResult<T> {
    let target = read(state, move |c| pr::diff_target(c, pr_id))
        .await?
        .ok_or(CmdError::NotFound)?;
    let git = state.git.clone();
    // The engine's token: honours a token GitHub already rejected.
    let token = state.sync.token();
    tauri::async_runtime::spawn_blocking(move || {
        let clone = git.ensure_commits(
            &target.owner,
            &target.name,
            token.as_deref(),
            &[&target.base_oid, &target.head_oid],
        )?;
        f(&git, &clone, &target)
    })
    .await
    .map_err(|e| CmdError::Other(format!("git task failed: {e}")))?
    .map_err(|e| match e {
        fl_git::GitError::Unauthorized => CmdError::Unauthorized,
        e => CmdError::Other(e.to_string()),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn diff_summary(state: State<'_, AppState>, pr_id: IssueId) -> CmdResult<DiffSummary> {
    with_clone(&state, pr_id, |git, clone, t| {
        git.diff_summary(clone, &t.base_oid, &t.head_oid)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn diff_file(
    state: State<'_, AppState>,
    pr_id: IssueId,
    path: String,
) -> CmdResult<DiffFile> {
    with_clone(&state, pr_id, move |git, clone, t| {
        git.diff_file(clone, &t.base_oid, &t.head_oid, &path)
    })
    .await
}
