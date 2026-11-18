//! Issue / PR list queries. Owner: agent 3 (home feed added in Phase 0).

use fl_types::{IssueFilter, IssueRow, Page, PageReq};
use rusqlite::types::Value;
use rusqlite::{Connection, params_from_iter};

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

/// Build the `issue_list` statement and its parameters. Each filter adds one
/// indexed predicate (author, assignee, label and review requests are
/// resolved through subqueries on indexed columns). `req` must be clamped.
pub fn issue_list_sql(f: &IssueFilter, req: PageReq) -> (String, Vec<Value>) {
    let mut conds = Vec::<&str>::new();
    let mut args = Vec::<Value>::new();
    if let Some(repo_id) = f.repo_id {
        conds.push("i.repo_id = ?");
        args.push(repo_id.into());
    }
    if f.review_requested {
        conds.push(
            "i.is_pr = 1 AND i.id IN (SELECT pr_id FROM pr_review_requests WHERE reviewer_login = \
             (SELECT json_extract(value_json, '$.login') FROM settings WHERE key = 'viewer'))",
        );
    } else if let Some(is_pr) = f.is_pr {
        conds.push("i.is_pr = ?");
        args.push(i64::from(is_pr).into());
    }
    if let Some(state) = f.state {
        conds.push("i.state = ?");
        args.push(state.as_str().to_owned().into());
    }
    if let Some(author) = &f.author {
        conds.push("i.author_id = (SELECT id FROM users WHERE login = ?)");
        args.push(author.clone().into());
    }
    if let Some(assignee) = &f.assignee {
        conds.push(
            "i.id IN (SELECT issue_id FROM issue_assignees WHERE user_id = \
             (SELECT id FROM users WHERE login = ?))",
        );
        args.push(assignee.clone().into());
    }
    if let Some(label) = &f.label {
        conds.push(
            "i.id IN (SELECT issue_id FROM issue_labels WHERE label_id IN \
             (SELECT id FROM labels WHERE name = ?))",
        );
        args.push(label.clone().into());
    }
    let where_ = if conds.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conds.join(" AND "))
    };
    let sql = format!(
        "SELECT {ISSUE_ROW_COLS} FROM issues i
         JOIN repos r ON r.id = i.repo_id
         LEFT JOIN users u ON u.id = i.author_id
         {where_} ORDER BY i.updated_at DESC LIMIT ? OFFSET ?"
    );
    args.push((req.limit + 1).into());
    args.push(req.offset.into());
    (sql, args)
}

/// Pull requests, Issues and Review requests views.
pub fn issue_list(
    conn: &Connection,
    filter: &IssueFilter,
    req: PageReq,
) -> rusqlite::Result<Page<IssueRow>> {
    let req = req.clamped();
    let (sql, args) = issue_list_sql(filter, req);
    let rows = conn
        .prepare_cached(&sql)?
        .query_map(params_from_iter(args), issue_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(page(rows, req))
}
