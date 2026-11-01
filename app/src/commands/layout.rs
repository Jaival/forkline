use fl_db::queries::layout;
use fl_types::CmdResult;
use serde::Serialize;
use specta::Type;
use tauri::State;

use super::db_err;
use crate::AppState;
use crate::bench::BenchState;

/// Everything the UI needs before its first paint, in one IPC call.
#[derive(Debug, Serialize, Type)]
pub struct Boot {
    /// Saved tabs + panes (UI-owned JSON), if any.
    pub layout_json: Option<String>,
    /// "system" | "light" | "dark" (JSON-encoded setting), if set.
    pub theme_json: Option<String>,
    pub bench: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn app_boot(state: State<'_, AppState>, bench: State<'_, BenchState>) -> CmdResult<Boot> {
    let (layout_json, theme_json) = state
        .db
        .read(|c| Ok((layout::get(c)?, layout::setting(c, "theme")?)))
        .map_err(db_err)?;
    Ok(Boot {
        layout_json,
        theme_json,
        bench: bench.enabled(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn layout_save(state: State<'_, AppState>, json: String) -> CmdResult<()> {
    state.db.write(|c| layout::save(c, &json)).map_err(db_err)
}

#[tauri::command]
#[specta::specta]
pub async fn setting_get(state: State<'_, AppState>, key: String) -> CmdResult<Option<String>> {
    state.db.read(|c| layout::setting(c, &key)).map_err(db_err)
}

#[tauri::command]
#[specta::specta]
pub async fn setting_set(
    state: State<'_, AppState>,
    key: String,
    value_json: String,
) -> CmdResult<()> {
    serde_json::from_str::<serde_json::Value>(&value_json)
        .map_err(|e| fl_types::CmdError::Other(format!("setting value is not JSON: {e}")))?;
    state
        .db
        .write(|c| layout::set_setting(c, &key, &value_json))
        .map_err(db_err)
}
