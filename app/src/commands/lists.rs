//! Owner: agent 3 (issue / PR lists).

use fl_db::queries::lists;
use fl_types::{CmdResult, IssueFilter, IssueRow, Page, PageReq};
use tauri::State;

use super::read;
use crate::AppState;

#[tauri::command]
#[specta::specta]
pub async fn home_feed(state: State<'_, AppState>, page: PageReq) -> CmdResult<Page<IssueRow>> {
    read(&state, move |c| lists::home_feed(c, page)).await
}

/// Pull requests, Issues and Review requests views.
#[tauri::command]
#[specta::specta]
pub async fn issue_list(
    state: State<'_, AppState>,
    filter: IssueFilter,
    page: PageReq,
) -> CmdResult<Page<IssueRow>> {
    read(&state, move |c| lists::issue_list(c, &filter, page)).await
}
