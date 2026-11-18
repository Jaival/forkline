//! Owner: agent 3 (notifications inbox).

use fl_db::queries::inbox;
use fl_types::{CmdResult, InboxFilter, InboxRow, Page, PageReq};
use tauri::State;

use super::read;
use crate::AppState;

/// Open (not done) notifications, newest first. Mark read / done go through
/// `action_enqueue`.
#[tauri::command]
#[specta::specta]
pub async fn inbox_list(
    state: State<'_, AppState>,
    filter: InboxFilter,
    page: PageReq,
) -> CmdResult<Page<InboxRow>> {
    read(&state, move |c| inbox::list(c, &filter, page)).await
}
