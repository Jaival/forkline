//! Owner: agent 3 (issue / PR lists).

use fl_db::queries::lists;
use fl_types::{CmdResult, IssueRow, Page, PageReq};
use tauri::State;

use super::{not_implemented, read};
use crate::AppState;

#[tauri::command]
#[specta::specta]
pub async fn home_feed(state: State<'_, AppState>, page: PageReq) -> CmdResult<Page<IssueRow>> {
    read(&state, move |c| lists::home_feed(c, page)).await
}

#[tauri::command]
#[specta::specta]
pub async fn issue_list() -> CmdResult<()> {
    not_implemented("issue_list")
}
