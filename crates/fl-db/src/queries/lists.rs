//! Issue / PR list queries. Owner: agent 3 (home feed added in Phase 0).

use fl_types::{IssueRow, Page, PageReq};
use rusqlite::Connection;

use super::{ISSUE_ROW_COLS, issue_row, page};

/// Home view: open items across all synced repos, most recently updated first.
pub fn home_feed(conn: &Connection, req: PageReq) -> rusqlite::Result<Page<IssueRow>> {
    let req = req.clamped();
    let sql = format!(
        "SELECT {ISSUE_ROW_COLS} FROM issues i INDEXED BY issues_updated
         JOIN repos r ON r.id = i.repo_id
         LEFT JOIN users u ON u.id = i.author_id
         WHERE i.state = 'open'
         ORDER BY i.updated_at DESC LIMIT ?1 OFFSET ?2"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let rows = stmt
        .query_map([req.limit + 1, req.offset], issue_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(page(rows, req))
}
