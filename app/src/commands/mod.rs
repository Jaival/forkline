//! Tauri commands, one file per feature. Each file is owned by one Phase 1
//! agent (see docs/HANDOFF.md). Register new commands in `lib.rs`.
//!
//! All DB commands are `async` and go through `read` / `write`, which run
//! the SQLite call on the blocking pool so neither the main (UI event loop)
//! thread nor an async worker waits on the database.

pub mod actions;
pub mod bench;
pub mod diff;
pub mod inbox;
pub mod issue;
pub mod layout;
pub mod lists;
pub mod pr;
pub mod repo;
pub mod search;

use fl_db::rusqlite::{self, Connection};
use fl_types::{CmdError, CmdResult, DataChanged};
use tauri::AppHandle;
use tauri_specta::Event;

use crate::AppState;
use crate::events::DataChangedEvent;

pub fn db_err(e: rusqlite::Error) -> CmdError {
    CmdError::Db(e.to_string())
}

/// Run `f` on a reader connection, off the async runtime.
pub async fn read<T: Send + 'static>(
    state: &AppState,
    f: impl FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db.read(f))
        .await
        .map_err(|e| CmdError::Other(format!("database task failed: {e}")))?
        .map_err(db_err)
}

/// Run `f` on the writer connection, off the async runtime.
pub async fn write<T: Send + 'static>(
    state: &AppState,
    f: impl FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
) -> CmdResult<T> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db.write(f))
        .await
        .map_err(|e| CmdError::Other(format!("database task failed: {e}")))?
        .map_err(db_err)
}

/// Tell the UI to invalidate queries under `scopes` after a write.
pub fn emit_changed(app: &AppHandle, scopes: &[&str]) -> CmdResult<()> {
    DataChangedEvent(DataChanged {
        scopes: scopes.iter().map(|s| (*s).to_owned()).collect(),
    })
    .emit(app)
    .map_err(|e| CmdError::Other(format!("emit data-changed: {e}")))
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn not_implemented<T>(what: &str) -> Result<T, CmdError> {
    Err(CmdError::NotImplemented(what.into()))
}
