//! Owner: agent 1 (sync status, action queue).

use fl_db::queries::sync;
use fl_types::{CmdResult, SyncStatus};
use tauri::State;

use super::{db_err, not_implemented};
use crate::AppState;

#[tauri::command]
#[specta::specta]
pub async fn sync_status(state: State<'_, AppState>) -> CmdResult<SyncStatus> {
    state.db.read(sync::status).map_err(db_err)
}

#[tauri::command]
#[specta::specta]
pub async fn action_enqueue(_kind: String, _payload_json: String) -> CmdResult<()> {
    not_implemented("action_enqueue")
}
