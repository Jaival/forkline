//! Owner: agent 5 (FTS5 search via fl-search).

use fl_types::{CmdResult, Page, PageReq, SearchHit};
use tauri::State;

use super::read;
use crate::AppState;

/// Free text plus operators (`is:pr`, `author:`, `label:`, `repo:`, ...).
#[tauri::command]
#[specta::specta]
pub async fn search(state: State<'_, AppState>, query: String, page: PageReq) -> CmdResult<Page<SearchHit>> {
    read(&state, move |c| fl_search::search(c, &query, page)).await
}
