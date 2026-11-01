//! Owner: agent 5 (FTS5 search via fl-search).

use fl_types::CmdResult;

use super::not_implemented;

#[tauri::command]
#[specta::specta]
pub async fn search(_query: String) -> CmdResult<()> {
    not_implemented("search")
}
