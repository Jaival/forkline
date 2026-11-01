use fl_db::queries::layout;
use fl_types::{CmdError, CmdResult};
use serde::Serialize;
use specta::Type;
use tauri::State;

use super::{read, write};
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
    let (layout_json, theme_json) =
        read(&state, |c| Ok((layout::get(c)?, layout::setting(c, "theme")?))).await?;
    Ok(Boot {
        layout_json,
        theme_json,
        bench: bench.enabled(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn layout_save(state: State<'_, AppState>, json: String) -> CmdResult<()> {
    write(&state, move |c| layout::save(c, &json)).await
}

#[tauri::command]
#[specta::specta]
pub async fn setting_get(state: State<'_, AppState>, key: String) -> CmdResult<Option<String>> {
    read(&state, move |c| layout::setting(c, &key)).await
}

#[tauri::command]
#[specta::specta]
pub async fn setting_set(
    state: State<'_, AppState>,
    key: String,
    value_json: String,
) -> CmdResult<()> {
    serde_json::from_str::<serde_json::Value>(&value_json)
        .map_err(|e| CmdError::Other(format!("setting value is not JSON: {e}")))?;
    write(&state, move |c| layout::set_setting(c, &key, &value_json)).await
}
