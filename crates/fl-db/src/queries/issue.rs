//! Issue detail queries. Owner: agent 3 (basic `get` added in Phase 0).

use fl_types::{IssueDetail, IssueId, Label};
use rusqlite::{Connection, OptionalExtension};

use super::{ISSUE_ROW_COLS, issue_row};

pub fn get(conn: &Connection, id: IssueId) -> rusqlite::Result<Option<IssueDetail>> {
    let sql = format!(
        "SELECT {ISSUE_ROW_COLS}, i.body_html, i.body_md, i.created_at FROM issues i
         JOIN repos r ON r.id = i.repo_id
         LEFT JOIN users u ON u.id = i.author_id
         WHERE i.id = ?1"
    );
    let detail = conn
        .prepare_cached(&sql)?
        .query_row([id], |r| {
            Ok(IssueDetail {
                row: issue_row(r)?,
                body_html: r.get(11)?,
                body_md: r.get(12)?,
                created_at: r.get(13)?,
                labels: Vec::new(),
            })
        })
        .optional()?;
    let Some(mut detail) = detail else {
        return Ok(None);
    };
    detail.labels = conn
        .prepare_cached(
            "SELECT l.id, l.name, l.color FROM issue_labels il
             JOIN labels l ON l.id = il.label_id WHERE il.issue_id = ?1 ORDER BY l.name",
        )?
        .query_map([id], |r| {
            Ok(Label {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Some(detail))
}

/// Opening an item clears its unread dot (GitHub state is not changed).
pub fn mark_viewed(conn: &Connection, id: IssueId, now: i64) -> rusqlite::Result<()> {
    conn.prepare_cached("UPDATE issues SET last_viewed_at = ?2 WHERE id = ?1")?
        .execute((id, now))?;
    Ok(())
}
