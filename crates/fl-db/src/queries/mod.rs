//! Read queries, one file per view. Each file is owned by one Phase 1 agent
//! (see docs/HANDOFF.md). Every list query must use an index; the
//! `query_plans` test checks this with EXPLAIN QUERY PLAN.

pub mod inbox;
pub mod issue;
pub mod layout;
pub mod lists;
pub mod pr;
pub mod repo;
pub mod sync;

use fl_types::{IssueRow, IssueState};
use rusqlite::Row;

/// Columns for `issue_row`. Join aliases: `i` = issues, `r` = repos, `u` = users.
pub const ISSUE_ROW_COLS: &str = "i.id, i.repo_id, r.owner || '/' || r.name, i.number, i.is_pr, i.state, \
     i.title, COALESCE(u.login, 'ghost'), i.comments_count, i.updated_at, \
     COALESCE(i.updated_at > i.last_viewed_at, 0)";

pub fn issue_row(r: &Row<'_>) -> rusqlite::Result<IssueRow> {
    let state: String = r.get(5)?;
    Ok(IssueRow {
        id: r.get(0)?,
        repo_id: r.get(1)?,
        repo: r.get(2)?,
        number: r.get(3)?,
        is_pr: r.get(4)?,
        state: IssueState::parse(&state).unwrap_or(IssueState::Open),
        title: r.get(6)?,
        author: r.get(7)?,
        comments: r.get(8)?,
        updated_at: r.get(9)?,
        unread: r.get(10)?,
    })
}

/// Build a `Page` from `limit + 1` fetched rows.
pub fn page<T>(mut items: Vec<T>, req: fl_types::PageReq) -> fl_types::Page<T> {
    let more = items.len() > req.limit as usize;
    items.truncate(req.limit as usize);
    fl_types::Page {
        items,
        next: more.then_some(req.offset + req.limit),
    }
}
