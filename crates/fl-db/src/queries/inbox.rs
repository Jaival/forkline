//! Inbox (notifications) queries. Owner: agent 3.

use fl_types::{InboxFilter, InboxRow, Page, PageReq};
use rusqlite::types::Value;
use rusqlite::{Connection, params_from_iter};

use super::page;

/// `req` must be clamped.
pub fn inbox_sql(f: &InboxFilter, req: PageReq) -> (String, Vec<Value>) {
    let mut args = Vec::<Value>::new();
    let mut where_ = String::from("n.done = 0");
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
                n.title, n.reason, n.unread, n.updated_at
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
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(page(rows, req))
}
