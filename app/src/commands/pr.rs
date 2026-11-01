//! Owner: agent 4 (PR view).

use fl_types::{CmdResult, IssueId};

use super::not_implemented;

#[tauri::command]
#[specta::specta]
pub async fn pr_get(_id: IssueId) -> CmdResult<()> {
    not_implemented("pr_get")
}
