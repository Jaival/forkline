//! Owner: agent 1 (sync status, action queue, sign-in).
//!
//! Thin wrappers over `fl_sync::Engine` (`AppState.sync`); the engine emits
//! `data-changed` itself.

use fl_types::{Action, ActionId, AuthPoll, AuthState, CmdResult, DeviceCode, FailedAction, SyncStatus};
use tauri::State;

use crate::AppState;

#[tauri::command]
#[specta::specta]
pub async fn sync_status(state: State<'_, AppState>) -> CmdResult<SyncStatus> {
    Ok(state.sync.status().await?)
}

/// Run a sync pass now.
#[tauri::command]
#[specta::specta]
pub async fn sync_now(state: State<'_, AppState>) -> CmdResult<()> {
    state.sync.wake();
    Ok(())
}

/// Validate, apply optimistically and queue a write. Returns at once; the
/// worker sends it to GitHub in the background.
#[tauri::command]
#[specta::specta]
pub async fn action_enqueue(state: State<'_, AppState>, action: Action) -> CmdResult<ActionId> {
    Ok(state.sync.enqueue(action).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn action_failed_list(state: State<'_, AppState>) -> CmdResult<Vec<FailedAction>> {
    Ok(state.sync.failed_actions().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn action_retry(state: State<'_, AppState>, id: ActionId) -> CmdResult<()> {
    Ok(state.sync.retry(id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn action_discard(state: State<'_, AppState>, id: ActionId) -> CmdResult<()> {
    Ok(state.sync.discard(id).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn auth_state(state: State<'_, AppState>) -> CmdResult<AuthState> {
    Ok(state.sync.auth_state().await?)
}

/// Start the device flow. The UI shows `user_code` and opens `verification_uri`.
#[tauri::command]
#[specta::specta]
pub async fn auth_start(state: State<'_, AppState>) -> CmdResult<DeviceCode> {
    Ok(state.sync.auth_start().await?)
}

/// One poll; call again after `interval` seconds while `pending`.
#[tauri::command]
#[specta::specta]
pub async fn auth_poll(state: State<'_, AppState>) -> CmdResult<AuthPoll> {
    Ok(state.sync.auth_poll().await?)
}

#[tauri::command]
#[specta::specta]
pub async fn auth_sign_out(state: State<'_, AppState>) -> CmdResult<()> {
    Ok(state.sync.sign_out().await?)
}
