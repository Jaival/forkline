//! Owner: agent 1 (sync status, action queue).

use fl_db::queries::sync;
use fl_types::{Action, ActionId, CmdResult, SyncStatus};
use tauri::State;

use super::{not_implemented, read};
use crate::AppState;

#[tauri::command]
#[specta::specta]
pub async fn sync_status(state: State<'_, AppState>) -> CmdResult<SyncStatus> {
    read(&state, sync::status).await
}

#[tauri::command]
#[specta::specta]
pub async fn action_enqueue(_action: Action) -> CmdResult<ActionId> {
    not_implemented("action_enqueue")
}
