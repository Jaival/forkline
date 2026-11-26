//! Work queue: the grouped list behind Inbox, Pull requests, Issues and
//! Review requests. See `fl_types::queue`.

use fl_types::{CheckState, IssueState, Page, PageReq, QueueGroup, QueueKind, QueueQuery, QueueRow, RowReason};
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

/// The signed-in user's login.
const VIEWER: &str = "(SELECT json_extract(value_json, '$.login') FROM settings WHERE key = 'viewer')";

/// Home: every open item of the Pull requests and Issues lists.
const HOME: &str = "i.state = 'open' AND (i.repo_id IN (SELECT id FROM repos WHERE pinned = 1) \
     OR i.id IN (SELECT issue_id FROM search_hits))";

/// Home group as an integer (index in `QueueGroup::ALL`): the first section
/// that fits. Reads the aliases `i`, `u` (author), `r`, `d` (pr_details).
fn home_rank() -> String {
    let mine = format!("u.login = {VIEWER}");
    let others = format!("COALESCE(u.login, '') <> {VIEWER}");
    let review_requested = format!(
        "(i.id IN (SELECT issue_id FROM search_hits WHERE query = 'review_requested') \
         OR i.id IN (SELECT pr_id FROM pr_review_requests WHERE reviewer_login = {VIEWER}))"
    );
    let my_repo = format!("(r.owner = {VIEWER} COLLATE NOCASE OR r.member = 1)");
    let assigned = format!(
        "(i.id IN (SELECT issue_id FROM search_hits WHERE query = 'assigned') \
         OR EXISTS (SELECT 1 FROM issue_assignees ia JOIN users au ON au.id = ia.user_id \
         WHERE ia.issue_id = i.id AND au.login = {VIEWER}))"
    );
    let mentioned = "(i.updated_at > COALESCE(i.last_viewed_at, 0) AND i.id IN (SELECT issue_id FROM \
         notifications WHERE unread = 1 AND done = 0 AND issue_id IS NOT NULL \
         AND reason IN ('mention', 'team_mention', 'assign')))";
    let draft = "COALESCE(d.is_draft, 0) = 1";
    format!(
        "CASE \
         WHEN i.is_pr = 1 AND {mine} AND (d.check_state IN ('FAILURE', 'ERROR') \
           OR d.review_decision = 'CHANGES_REQUESTED' OR d.mergeable = 'CONFLICTING') THEN 6 \
         WHEN i.is_pr = 1 AND {others} AND ({review_requested} OR ({my_repo} AND NOT {draft})) THEN 7 \
         WHEN {assigned} OR {mentioned} THEN 8 \
         WHEN i.is_pr = 1 AND {mine} AND NOT {draft} AND COALESCE(d.check_state, 'SUCCESS') = 'SUCCESS' \
           AND COALESCE(d.review_decision, '') IN ('', 'APPROVED') THEN 9 \
         WHEN i.is_pr = 1 AND {mine} THEN 10 \
         ELSE 11 END"
    )
}

/// Columns 13-16 of a list row: what changed since the item was last opened,
/// and whether the author is a GitHub App. Over `i`, `u`, `d`.
const CHANGE_COLS: &str = "d.review_decision, \
     CASE WHEN i.last_viewed_at IS NOT NULL AND i.updated_at > i.last_viewed_at THEN \
       (SELECT count(*) FROM comments c WHERE c.issue_id = i.id AND c.created_at > i.last_viewed_at) \
       ELSE 0 END, \
     CASE WHEN i.is_pr = 1 AND i.last_viewed_at IS NOT NULL AND i.updated_at > i.last_viewed_at THEN \
       EXISTS (SELECT 1 FROM pr_commits pc JOIN commits cm ON cm.repo_id = i.repo_id AND cm.oid = pc.oid \
       WHERE pc.pr_id = i.id AND cm.authored_at > i.last_viewed_at) ELSE 0 END, \
     COALESCE(u.avatar_url LIKE '%/in/%', 0)";

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
    // A filter and the Home groups read the joined tables.
    let joins = joins || cond.is_some() || q.kind == QueueKind::Home;
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
                QueueKind::Home => HOME,
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
                 d.check_state, n.updated_at, n.unread, {CHANGE_COLS} \
                 FROM {from_where} ORDER BY {group_first}n.updated_at DESC, n.id LIMIT ? OFFSET ?"
            )
        }
        QueueKind::Home => {
            let group_first = if q.flat { "" } else { "2, " };
            format!(
                "SELECT CAST(i.id AS TEXT), {}, i.id, i.repo_id, r.owner || '/' || r.name, i.number, \
                 i.is_pr, i.state, i.title, u.login, d.check_state, i.updated_at, \
                 i.updated_at > COALESCE(i.last_viewed_at, 0), {CHANGE_COLS} \
                 FROM {from_where} ORDER BY {group_first}i.updated_at DESC, i.id LIMIT ? OFFSET ?",
                home_rank()
            )
        }
        kind => {
            let rank = if kind == QueueKind::Reviews { 0 } else { 5 };
            format!(
                "SELECT CAST(i.id AS TEXT), {rank}, i.id, i.repo_id, r.owner || '/' || r.name, i.number, \
                 i.is_pr, i.state, i.title, u.login, d.check_state, i.updated_at, \
                 i.updated_at > COALESCE(i.last_viewed_at, 0), {CHANGE_COLS} \
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
    let checks = checks.as_deref().and_then(CheckState::parse);
    let unread: bool = r.get(12)?;
    let review: Option<String> = r.get(13)?;
    let reason = RowReason::pick(unread, checks, review.as_deref(), r.get(14)?, r.get(15)?);
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
        checks,
        updated_at: r.get(11)?,
        unread,
        reason,
        bot: r.get(16)?,
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
        QueueKind::Home => format!("SELECT {}, COUNT(*) FROM {from_where} GROUP BY 1 ORDER BY 1", home_rank()),
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

/// Inbox only: the thread ids of the notifications in `q` (and `cond`), in
/// one group or all, for "Done all". `closed_only`: only those whose issue or
/// PR is merged or closed.
pub fn inbox_keys(
    conn: &Connection,
    q: &QueueQuery,
    cond: Option<&Cond>,
    group: Option<QueueGroup>,
    closed_only: bool,
) -> rusqlite::Result<Vec<String>> {
    debug_assert_eq!(q.kind, QueueKind::Inbox);
    let (from_where, mut args) = from_where(q, cond, true);
    let mut sql = format!("SELECT n.id FROM {from_where}");
    if let Some(group) = group {
        let rank = QueueGroup::ALL.iter().position(|g| *g == group).unwrap_or_default();
        sql.push_str(&format!(" AND {INBOX_RANK} = ?"));
        args.push(i64::try_from(rank).unwrap_or_default().into());
    }
    if closed_only {
        sql.push_str(" AND i.state IN ('closed', 'merged')");
    }
    conn.prepare(&sql)?.query_map(params_from_iter(args), |r| r.get(0))?.collect()
}
