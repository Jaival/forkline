//! Owner: agent 3 (notifications inbox).

use fl_types::CmdResult;

use super::not_implemented;

#[tauri::command]
#[specta::specta]
pub async fn inbox_list() -> CmdResult<()> {
    not_implemented("inbox_list")
}
