//! Work queue: the grouped list behind Inbox, Pull requests, Issues and
//! Review requests. See `fl_types::queue`.

use fl_types::{CheckState, IssueState, Page, PageReq, QueueGroup, QueueKind, QueueQuery, QueueRow};
use rusqlite::types::Value;
use rusqlite::{Connection, Row, params_from_iter};

/// Inbox group as an integer: the index of the group in `QueueGroup::ALL`.
const INBOX_RANK: &str = "CASE n.reason WHEN 'review_requested' THEN 0 \
     WHEN 'mention' THEN 1 WHEN 'team_mention' THEN 1 WHEN 'assign' THEN 2 \
     WHEN 'author' THEN 3 ELSE 4 END";

/// Review requests: the viewer's search hits, plus local requests in
/// synced repos (deep sync and fake data).
const VIEWER_REVIEW: &str = "i.is_pr = 1 AND i.state = 'open' AND (i.id IN (SELECT issue_id FROM \
     search_hits WHERE query = 'review_requested') OR i.id IN (SELECT pr_id FROM \
     pr_review_requests WHERE reviewer_login = \
     (SELECT json_extract(value_json, '$.login') FROM settings WHERE key = 'viewer')))";

/// Pull requests and Issues: items of pinned repos and the viewer's search
/// hits. Other cached items (a repo that was unpinned) can be stale.
const PULLS: &str = "i.is_pr = 1 AND i.state = 'open' AND (i.repo_id IN (SELECT id FROM repos WHERE \
     pinned = 1) OR i.id IN (SELECT issue_id FROM search_hits))";
const ISSUES: &str = "i.is_pr = 0 AND i.state = 'open' AND (i.repo_id IN (SELECT id FROM repos WHERE \
     pinned = 1) OR i.id IN (SELECT issue_id FROM search_hits))";

/// An extra SQL condition on a queue (the `/` filter), built by
/// `fl_search::queue_filter`. It may read the aliases `i` (issues), `r`
/// (repos) and `u` (author); in the Inbox `i` and `u` are left joins, so a
/// notification without a cached issue fails any condition on them. Bind
/// with anonymous `?` only, in the order of `args`.
#[derive(Debug, Clone, PartialEq)]
pub struct Cond {
    pub sql: String,
    pub args: Vec<Value>,
}

/// FROM and WHERE, shared by the list and the counts.
fn from_where(q: &QueueQuery, cond: Option<&Cond>, joins: bool) -> (String, Vec<Value>) {
    // A filter reads the joined tables.
    let joins = joins || cond.is_some();
    let mut args = Vec::new();
    let (from, mut conds, alias) = match q.kind {
        QueueKind::Inbox => (
            if joins {
                "notifications n JOIN repos r ON r.id = n.repo_id \
                 LEFT JOIN issues i ON i.id = n.issue_id \
                 LEFT JOIN users u ON u.id = i.author_id \
                 LEFT JOIN pr_details d ON d.issue_id = n.issue_id"
            } else {
                "notifications n"
            },
            vec![super::inbox::IN_INBOX],
            "n",
        ),
        kind => (
            if joins {
                "issues i JOIN repos r ON r.id = i.repo_id \
                 LEFT JOIN users u ON u.id = i.author_id \
                 LEFT JOIN pr_details d ON d.issue_id = i.id"
            } else {
                "issues i"
            },
            vec![match kind {
                QueueKind::Pulls => PULLS,
                QueueKind::Issues => ISSUES,
                _ => VIEWER_REVIEW,
            }],
            "i",
        ),
    };
    let repo_cond = format!("{alias}.repo_id = ?");
    if let Some(repo_id) = q.repo_id {
        conds.push(&repo_cond);
        args.push(repo_id.into());
    }
    if let Some(cond) = cond {
        conds.push(&cond.sql);
        args.extend(cond.args.iter().cloned());
    }
    (format!("{from} WHERE {}", conds.join(" AND ")), args)
}

/// Build the list statement and its parameters. `req` must be clamped.
pub fn queue_sql(q: &QueueQuery, cond: Option<&Cond>, req: PageReq) -> (String, Vec<Value>) {
    let (from_where, mut args) = from_where(q, cond, true);
    let sql = match q.kind {
        QueueKind::Inbox => {
            let group_first = if q.flat { "" } else { "2, " };
            format!(
                "SELECT n.id, {INBOX_RANK}, n.issue_id, n.repo_id, r.owner || '/' || r.name, i.number, \
                 COALESCE(i.is_pr, n.subject_type = 'PullRequest'), i.state, n.title, u.login, \
                 d.check_state, n.updated_at, n.unread \
                 FROM {from_where} ORDER BY {group_first}n.updated_at DESC, n.id LIMIT ? OFFSET ?"
            )
        }
        kind => {
            let rank = if kind == QueueKind::Reviews { 0 } else { 5 };
            format!(
                "SELECT CAST(i.id AS TEXT), {rank}, i.id, i.repo_id, r.owner || '/' || r.name, i.number, \
                 i.is_pr, i.state, i.title, u.login, d.check_state, i.updated_at, \
                 i.updated_at > COALESCE(i.last_viewed_at, 0) \
                 FROM {from_where} ORDER BY i.updated_at DESC, i.id LIMIT ? OFFSET ?"
            )
        }
    };
    args.push(i64::from(req.limit + 1).into());
    args.push(i64::from(req.offset).into());
    (sql, args)
}

fn group_at(rank: i64) -> QueueGroup {
    QueueGroup::ALL[rank as usize]
}

fn queue_row(r: &Row<'_>) -> rusqlite::Result<QueueRow> {
    let state: Option<String> = r.get(7)?;
    let checks: Option<String> = r.get(10)?;
    Ok(QueueRow {
        key: r.get(0)?,
        group: group_at(r.get(1)?),
        issue_id: r.get(2)?,
        repo_id: r.get(3)?,
        repo: r.get(4)?,
        number: r.get(5)?,
        is_pr: r.get(6)?,
        state: state.as_deref().and_then(IssueState::parse),
        title: r.get(8)?,
        author: r.get(9)?,
        checks: checks.as_deref().and_then(CheckState::parse),
        updated_at: r.get(11)?,
        unread: r.get(12)?,
    })
}

pub fn page(conn: &Connection, q: &QueueQuery, cond: Option<&Cond>, req: PageReq) -> rusqlite::Result<Page<QueueRow>> {
    let req = req.clamped();
    let (sql, args) = queue_sql(q, cond, req);
    let rows = conn
        .prepare_cached(&sql)?
        .query_map(params_from_iter(args), queue_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(super::page(rows, req))
}

/// Build the counts statement: rows of `(group rank, count)`.
pub fn counts_sql(q: &QueueQuery, cond: Option<&Cond>) -> (String, Vec<Value>) {
    let (from_where, args) = from_where(q, cond, false);
    let sql = match q.kind {
        QueueKind::Inbox => {
            format!("SELECT {INBOX_RANK}, COUNT(*) FROM {from_where} GROUP BY 1 ORDER BY 1")
        }
        kind => {
            let rank = if kind == QueueKind::Reviews { 0 } else { 5 };
            format!("SELECT {rank}, COUNT(*) FROM {from_where}")
        }
    };
    (sql, args)
}

/// Rows per group, display order, groups with zero rows left out.
pub fn counts(conn: &Connection, q: &QueueQuery, cond: Option<&Cond>) -> rusqlite::Result<Vec<(QueueGroup, u32)>> {
    let (sql, args) = counts_sql(q, cond);
    let rows = conn
        .prepare_cached(&sql)?
        .query_map(params_from_iter(args), |r| Ok((group_at(r.get(0)?), r.get::<_, u32>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows.into_iter().filter(|&(_, n)| n > 0).collect())
}
