//! Owner: agent 4 (diff viewer; reads diffs from fl-git, built by agent 5).

use fl_types::{CmdResult, IssueId};

use super::not_implemented;

#[tauri::command]
#[specta::specta]
pub async fn diff_get(_pr_id: IssueId) -> CmdResult<()> {
    not_implemented("diff_get")
}
