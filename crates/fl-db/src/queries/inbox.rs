//! Inbox (notifications) queries. Owner: agent 3.

use fl_types::{InboxFilter, InboxRow, Page, PageReq};
use rusqlite::types::Value;
use rusqlite::{Connection, params_from_iter};

use super::page;

/// Condition for a notification that is in the inbox (alias `n`): not done, not snoozed.
pub const IN_INBOX: &str = "n.done = 0 \
     AND (n.done_at IS NULL OR n.updated_at > n.done_at) \
     AND (n.snoozed_until IS NULL OR n.snoozed_until <= CAST(strftime('%s','now') AS INTEGER))";

/// `req` must be clamped.
pub fn inbox_sql(f: &InboxFilter, req: PageReq) -> (String, Vec<Value>) {
    let mut args = Vec::<Value>::new();
    let mut where_ = String::from(IN_INBOX);
    if let Some(repo_id) = f.repo_id {
        where_.push_str(" AND n.repo_id = ?");
        args.push(repo_id.into());
    }
    if f.unread_only {
        where_.push_str(" AND n.unread = 1");
    }
    let order = if f.grouped {
        "n.repo_id, n.updated_at DESC"
    } else {
        "n.updated_at DESC"
    };
    args.push((req.limit + 1).into());
    args.push(req.offset.into());
    let sql = format!(
        "SELECT n.id, n.repo_id, r.owner || '/' || r.name, n.subject_type, n.issue_id,
                n.title, n.reason, n.unread, n.updated_at,
                EXISTS (SELECT 1 FROM pending_fields p WHERE p.table_name = 'notifications'
                        AND p.row_id = CAST(n.id AS INTEGER))
         FROM notifications n JOIN repos r ON r.id = n.repo_id
         WHERE {where_} ORDER BY {order} LIMIT ? OFFSET ?"
    );
    (sql, args)
}

pub fn list(conn: &Connection, f: &InboxFilter, req: PageReq) -> rusqlite::Result<Page<InboxRow>> {
    let req = req.clamped();
    let (sql, args) = inbox_sql(f, req);
    let rows = conn
        .prepare_cached(&sql)?
        .query_map(params_from_iter(args), |r| {
            let subject_type: String = r.get(3)?;
            Ok(InboxRow {
                id: r.get(0)?,
                repo_id: r.get(1)?,
                repo: r.get(2)?,
                is_pr: subject_type == "PullRequest",
                subject_type,
                issue_id: r.get(4)?,
                title: r.get(5)?,
                reason: r.get(6)?,
                unread: r.get(7)?,
                updated_at: r.get(8)?,
                pending: r.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(page(rows, req))
}

/// Unread notifications in the inbox (the rail badge).
pub fn unread_count(conn: &Connection) -> rusqlite::Result<u32> {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM notifications n WHERE {IN_INBOX} AND n.unread = 1"),
        [],
        |r| r.get(0),
    )
}

/// Mark done locally; it returns only after a later update.
pub fn set_done(conn: &Connection, id: &str, now: i64) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE notifications SET done_at = MAX(?2, updated_at), snoozed_until = NULL WHERE id = ?1",
        rusqlite::params![id, now],
    )?;
    Ok(n > 0)
}

/// Hide until the unix time `until`.
pub fn snooze(conn: &Connection, id: &str, until: i64) -> rusqlite::Result<bool> {
    let n = conn.execute(
        "UPDATE notifications SET snoozed_until = ?2 WHERE id = ?1",
        rusqlite::params![id, until],
    )?;
    Ok(n > 0)
}
