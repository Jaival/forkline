//! Optimistic write actions.
//!
//! `enqueue` validates an `Action`, applies it to SQLite at once, records the
//! old values in `pending_fields` and queues it. The worker (`engine`) then
//! calls `claim` → `plan` → GitHub → `complete` or `fail`.
//!
//! `pending_fields` rows:
//! - scalar / set fields (`issues.state`, `issues.labels`, ...): restore
//!   `old_value_json` on final failure;
//! - field `row`: an optimistic local row with a negative id; deleted on
//!   final failure. Once the action succeeds the row stays (comments get
//!   their GitHub id) until deep sync replaces it.

use fl_types::{Action, ActionId, FailedAction, IssueState, MergeMethod, ReviewEvent};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Value, json};

use crate::{Error, Result};

/// Retries for 5xx answers before the action fails for good. Network errors
/// and rate limits retry without a limit (offline use).
pub const MAX_ATTEMPTS: i64 = 8;
const MAX_BACKOFF: i64 = 300;
const MAX_BODY: usize = 65_536;

/// What `enqueue` did: the new id and the `data-changed` scopes to emit.
pub struct Enqueued {
    pub id: ActionId,
    pub scopes: &'static [&'static str],
}

pub fn scopes(action: &Action) -> &'static [&'static str] {
    match action {
        Action::NotificationRead { .. } | Action::NotificationDone { .. } => &["inbox", "sync"],
        _ => &["issues", "pr", "inbox", "sync"],
    }
}

/// Validate, apply optimistically and queue. One transaction.
pub fn enqueue(conn: &mut Connection, action: &Action, now: i64) -> Result<Enqueued> {
    let tx = conn.transaction()?;
    let (table, target, summary) = describe(&tx, action)?;
    tx.execute(
        "INSERT INTO action_queue (kind, payload_json, target_table, target_id, next_attempt_at, created_at, summary)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
        params![kind(action), json_of(action), table, target, now, summary],
    )?;
    let id = tx.last_insert_rowid();
    apply(&tx, id, action, now)?;
    tx.commit()?;
    Ok(Enqueued { id, scopes: scopes(action) })
}

/// Re-apply a failed action and queue it again with fresh attempts.
pub fn retry(conn: &mut Connection, id: ActionId, now: i64) -> Result<&'static [&'static str]> {
    let tx = conn.transaction()?;
    let action = load(&tx, id, "failed")?;
    describe(&tx, &action)?;
    apply(&tx, id, &action, now)?;
    tx.execute(
        "UPDATE action_queue SET state = 'pending', attempts = 0, next_attempt_at = ?2, last_error = NULL
         WHERE id = ?1",
        params![id, now],
    )?;
    tx.commit()?;
    Ok(scopes(&action))
}

/// Drop a failed action (its change was already rolled back).
pub fn discard(conn: &Connection, id: ActionId) -> Result<()> {
    let n = conn.execute("DELETE FROM action_queue WHERE id = ?1 AND state = 'failed'", [id])?;
    if n == 0 {
        return Err(Error::Invalid("no failed action with that id".into()));
    }
    Ok(())
}

pub fn failed(conn: &Connection) -> rusqlite::Result<Vec<FailedAction>> {
    conn.prepare_cached(
        "SELECT id, summary, COALESCE(last_error, ''), created_at FROM action_queue
         WHERE state = 'failed' ORDER BY next_attempt_at, id LIMIT 100",
    )?
    .query_map([], |r| {
        Ok(FailedAction { id: r.get(0)?, summary: r.get(1)?, error: r.get(2)?, created_at: r.get(3)? })
    })?
    .collect()
}

/// After a crash nothing is in flight.
pub fn reset_in_flight(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("UPDATE action_queue SET in_flight = 0 WHERE in_flight = 1", [])?;
    Ok(())
}

/// Mark the next due action as in flight and return it.
pub fn claim(conn: &Connection, now: i64) -> Result<Option<(ActionId, Action)>> {
    let Some(id) = conn
        .prepare_cached(
            "SELECT id FROM action_queue WHERE state = 'pending' AND next_attempt_at <= ?1
             ORDER BY next_attempt_at, id LIMIT 1",
        )?
        .query_row([now], |r| r.get::<_, i64>(0))
        .optional()?
    else {
        return Ok(None);
    };
    conn.execute("UPDATE action_queue SET in_flight = 1 WHERE id = ?1", [id])?;
    Ok(Some((id, load(conn, id, "pending")?)))
}

fn load(conn: &Connection, id: ActionId, state: &str) -> Result<Action> {
    let payload: String = conn
        .query_row(
            "SELECT payload_json FROM action_queue WHERE id = ?1 AND state = ?2",
            params![id, state],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| Error::Invalid(format!("no {state} action with that id")))?;
    serde_json::from_str(&payload).map_err(|e| Error::Invalid(format!("bad action payload: {e}")))
}

/// One HTTP request for an action.
#[derive(Debug, Clone)]
pub enum Request {
    Rest { method: reqwest::Method, path: String, body: Option<Value> },
    Graphql { query: &'static str, vars: Value },
}

/// Build the GitHub request from the current DB state (names, numbers,
/// node ids). Runs after `claim`, so the target rows exist.
pub fn plan(conn: &Connection, action: &Action) -> Result<Request> {
    use reqwest::Method;
    let issue = |id: i64| -> Result<(String, i64, String)> {
        let (repo, number, head): (String, i64, Option<String>) = conn.query_row(
            "SELECT r.owner || '/' || r.name, i.number, p.head_oid FROM issues i
             JOIN repos r ON r.id = i.repo_id LEFT JOIN pr_details p ON p.issue_id = i.id
             WHERE i.id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        Ok((repo, number, head.unwrap_or_default()))
    };
    let rest = |method: Method, path: String, body: Option<Value>| Request::Rest { method, path, body };
    Ok(match action {
        Action::Comment { issue_id, body_md } => {
            let (repo, n, _) = issue(*issue_id)?;
            rest(Method::POST, format!("/repos/{repo}/issues/{n}/comments"), Some(json!({ "body": body_md })))
        }
        Action::SetState { issue_id, state } => {
            let (repo, n, _) = issue(*issue_id)?;
            rest(Method::PATCH, format!("/repos/{repo}/issues/{n}"), Some(json!({ "state": state.as_str() })))
        }
        Action::SetLabels { issue_id, label_ids } => {
            let (repo, n, _) = issue(*issue_id)?;
            let names = names(conn, "labels", "name", label_ids)?;
            rest(Method::PUT, format!("/repos/{repo}/issues/{n}/labels"), Some(json!({ "labels": names })))
        }
        Action::SetAssignees { issue_id, user_ids } => {
            let (repo, n, _) = issue(*issue_id)?;
            let logins = names(conn, "users", "login", user_ids)?;
            rest(Method::PATCH, format!("/repos/{repo}/issues/{n}"), Some(json!({ "assignees": logins })))
        }
        Action::SubmitReview { pr_id, event, body_md, comments } => {
            let (repo, n, head) = issue(*pr_id)?;
            let comments: Vec<Value> = comments
                .iter()
                .map(|c| {
                    let mut v = json!({ "path": c.path, "line": c.line, "side": c.side, "body": c.body_md });
                    if let Some(start) = c.start_line {
                        v["start_line"] = json!(start);
                        v["start_side"] = json!(c.side);
                    }
                    v
                })
                .collect();
            let mut body = json!({ "event": event, "body": body_md, "comments": comments });
            if !head.is_empty() {
                body["commit_id"] = json!(head);
            }
            rest(Method::POST, format!("/repos/{repo}/pulls/{n}/reviews"), Some(body))
        }
        Action::ReplyThread { thread_id, body_md } => Request::Graphql {
            query: "mutation($id:ID!,$body:String!){addPullRequestReviewThreadReply(input:{pullRequestReviewThreadId:$id,body:$body}){comment{databaseId}}}",
            vars: json!({ "id": thread_node(conn, *thread_id)?, "body": body_md }),
        },
        Action::ResolveThread { thread_id, resolved } => Request::Graphql {
            query: if *resolved {
                "mutation($id:ID!){resolveReviewThread(input:{threadId:$id}){thread{isResolved}}}"
            } else {
                "mutation($id:ID!){unresolveReviewThread(input:{threadId:$id}){thread{isResolved}}}"
            },
            vars: json!({ "id": thread_node(conn, *thread_id)? }),
        },
        Action::Merge { pr_id, method } => {
            let (repo, n, head) = issue(*pr_id)?;
            let mut body = json!({ "merge_method": method });
            if !head.is_empty() {
                body["sha"] = json!(head);
            }
            rest(Method::PUT, format!("/repos/{repo}/pulls/{n}/merge"), Some(body))
        }
        Action::NotificationRead { id } => rest(Method::PATCH, format!("/notifications/threads/{id}"), None),
        Action::NotificationDone { id } => rest(Method::DELETE, format!("/notifications/threads/{id}"), None),
    })
}

/// GitHub accepted the action. Swap a local comment id for the real one,
/// re-index search, and drop the queue row (cascades `pending_fields`).
pub fn complete(conn: &mut Connection, id: ActionId, action: &Action, response: &Value) -> Result<()> {
    let tx = conn.transaction()?;
    if let Action::Comment { issue_id, .. } = action
        && let Some(server_id) = response["id"].as_i64()
    {
        let local: Option<i64> = tx
            .query_row(
                "SELECT row_id FROM pending_fields WHERE action_id = ?1 AND table_name = 'comments' AND field = 'row'",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(local) = local {
            tx.execute("DELETE FROM pending_fields WHERE action_id = ?1", [id])?;
            // Sync may already have stored the real comment.
            tx.execute("DELETE FROM comments WHERE id = ?1 AND ?1 <> ?2", params![server_id, local])?;
            tx.execute(
                "UPDATE comments SET id = ?1, body_html = COALESCE(?2, body_html) WHERE id = ?3",
                params![server_id, response["body_html"].as_str(), local],
            )?;
            fl_search::index_issue(&tx, *issue_id)?;
        }
    }
    tx.execute("DELETE FROM action_queue WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(())
}

/// The send failed. Retryable errors reschedule with backoff; others (and
/// 5xx after `MAX_ATTEMPTS`) roll back and keep the row as `failed`.
/// Returns true when the action failed for good.
pub fn fail(conn: &mut Connection, id: ActionId, err: &Error, now: i64) -> Result<bool> {
    let tx = conn.transaction()?;
    let attempts: i64 = tx.query_row("SELECT attempts + 1 FROM action_queue WHERE id = ?1", [id], |r| r.get(0))?;
    let message = err.to_string();
    let final_fail = match err {
        Error::Unauthorized => {
            // Wait for sign-in; not the action's fault.
            tx.execute("UPDATE action_queue SET in_flight = 0 WHERE id = ?1", [id])?;
            tx.commit()?;
            return Ok(false);
        }
        Error::RateLimited { until } => {
            tx.execute(
                "UPDATE action_queue SET in_flight = 0, next_attempt_at = ?2, last_error = ?3 WHERE id = ?1",
                params![id, until, message],
            )?;
            tx.commit()?;
            return Ok(false);
        }
        Error::Network(_) => false,
        e => !e.is_retryable() || attempts >= MAX_ATTEMPTS,
    };
    if final_fail {
        rollback(&tx, id)?;
        tx.execute(
            "UPDATE action_queue SET state = 'failed', in_flight = 0, attempts = ?2, last_error = ?3 WHERE id = ?1",
            params![id, attempts, message],
        )?;
    } else {
        let backoff = (1_i64 << attempts.min(9)).min(MAX_BACKOFF);
        tx.execute(
            "UPDATE action_queue SET in_flight = 0, attempts = ?2, next_attempt_at = ?3, last_error = ?4 WHERE id = ?1",
            params![id, attempts, now + backoff, message],
        )?;
    }
    tx.commit()?;
    Ok(final_fail)
}

/// Restore every field this action still owns and delete its local rows.
fn rollback(tx: &Transaction<'_>, id: ActionId) -> Result<()> {
    let rows: Vec<(String, i64, String, Option<String>)> = tx
        .prepare("SELECT table_name, row_id, field, old_value_json FROM pending_fields WHERE action_id = ?1")?
        .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (table, row, field, old) in rows {
        let old: Value = old.as_deref().map_or(Value::Null, |s| serde_json::from_str(s).unwrap_or(Value::Null));
        match (table.as_str(), field.as_str()) {
            (t @ ("comments" | "reviews" | "review_threads" | "review_comments"), "row") => {
                tx.execute(&format!("DELETE FROM {t} WHERE id = ?1"), [row])?;
            }
            ("issues", "labels") => set_links(tx, "issue_labels", "label_id", row, &ids_of(&old))?,
            ("issues", "assignees") => set_links(tx, "issue_assignees", "user_id", row, &ids_of(&old))?,
            (t @ ("issues" | "review_threads" | "notifications"), f @ ("state" | "is_resolved" | "unread" | "done")) => {
                let v = match old {
                    Value::String(s) => rusqlite::types::Value::Text(s),
                    Value::Number(n) => rusqlite::types::Value::Integer(n.as_i64().unwrap_or(0)),
                    _ => continue,
                };
                tx.execute(&format!("UPDATE {t} SET {f} = ?1 WHERE id = ?2"), params![v, row])?;
            }
            _ => {}
        }
    }
    tx.execute("DELETE FROM pending_fields WHERE action_id = ?1", [id])?;
    Ok(())
}

/// True when sync must not overwrite `table.field` of `row`.
pub fn is_pending(conn: &Connection, table: &str, row: i64, field: &str) -> rusqlite::Result<bool> {
    conn.prepare_cached("SELECT 1 FROM pending_fields WHERE table_name = ?1 AND row_id = ?2 AND field = ?3")?
        .exists(params![table, row, field])
}

// ---- enqueue helpers -------------------------------------------------------

fn kind(a: &Action) -> &'static str {
    match a {
        Action::Comment { .. } => "comment",
        Action::SetState { .. } => "set_state",
        Action::SetLabels { .. } => "set_labels",
        Action::SetAssignees { .. } => "set_assignees",
        Action::SubmitReview { .. } => "submit_review",
        Action::ReplyThread { .. } => "reply_thread",
        Action::ResolveThread { .. } => "resolve_thread",
        Action::Merge { .. } => "merge",
        Action::NotificationRead { .. } => "notification_read",
        Action::NotificationDone { .. } => "notification_done",
    }
}

fn json_of(a: &Action) -> String {
    serde_json::to_string(a).expect("Action serializes")
}

fn invalid<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error::Invalid(msg.into()))
}

fn check_body(body: &str, what: &str) -> Result<()> {
    if body.trim().is_empty() {
        return invalid(format!("{what} must not be empty"));
    }
    if body.len() > MAX_BODY {
        return invalid(format!("{what} is too long"));
    }
    Ok(())
}

struct IssueRef {
    repo_id: i64,
    label: String,
    is_pr: bool,
    state: String,
}

fn issue_ref(conn: &Connection, id: i64) -> Result<IssueRef> {
    conn.query_row(
        "SELECT i.repo_id, r.owner || '/' || r.name || '#' || i.number, i.is_pr, i.state
         FROM issues i JOIN repos r ON r.id = i.repo_id WHERE i.id = ?1",
        [id],
        |r| Ok(IssueRef { repo_id: r.get(0)?, label: r.get(1)?, is_pr: r.get(2)?, state: r.get(3)? }),
    )
    .optional()?
    .ok_or_else(|| Error::Invalid("issue not found".into()))
}

fn pr_ref(conn: &Connection, id: i64) -> Result<IssueRef> {
    let r = issue_ref(conn, id)?;
    if !r.is_pr {
        return invalid("not a pull request");
    }
    Ok(r)
}

/// Thread exists and is known to GitHub (not a local optimistic thread).
fn thread_ref(conn: &Connection, id: i64) -> Result<IssueRef> {
    let pr: i64 = conn
        .query_row("SELECT pr_id FROM review_threads WHERE id = ?1 AND node_id <> ''", [id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| Error::Invalid("review thread not found or not sent yet".into()))?;
    issue_ref(conn, pr)
}

fn thread_node(conn: &Connection, id: i64) -> Result<String> {
    Ok(conn.query_row("SELECT node_id FROM review_threads WHERE id = ?1", [id], |r| r.get(0))?)
}

fn notification_id(id: &str) -> Result<i64> {
    id.parse().map_err(|_| Error::Invalid("bad notification id".into()))
}

fn names(conn: &Connection, table: &str, col: &str, ids: &[i64]) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("SELECT {col} FROM {table} WHERE id = ?1"))?;
    ids.iter().map(|id| Ok(stmt.query_row([id], |r| r.get(0))?)).collect()
}

/// Validate and return `(target_table, target_id, summary)`.
fn describe(conn: &Connection, action: &Action) -> Result<(&'static str, i64, String)> {
    Ok(match action {
        Action::Comment { issue_id, body_md } => {
            check_body(body_md, "comment")?;
            ("issues", *issue_id, format!("Comment on {}", issue_ref(conn, *issue_id)?.label))
        }
        Action::SetState { issue_id, state } => {
            let r = issue_ref(conn, *issue_id)?;
            if *state == IssueState::Merged {
                return invalid("use Merge to merge a pull request");
            }
            if r.state == "merged" {
                return invalid("a merged pull request cannot change state");
            }
            let verb = if *state == IssueState::Open { "Reopen" } else { "Close" };
            ("issues", *issue_id, format!("{verb} {}", r.label))
        }
        Action::SetLabels { issue_id, label_ids } => {
            let r = issue_ref(conn, *issue_id)?;
            let mut stmt = conn.prepare_cached("SELECT 1 FROM labels WHERE id = ?1 AND repo_id = ?2")?;
            for l in label_ids {
                if !stmt.exists(params![l, r.repo_id])? {
                    return invalid("label does not belong to this repository");
                }
            }
            ("issues", *issue_id, format!("Edit labels on {}", r.label))
        }
        Action::SetAssignees { issue_id, user_ids } => {
            let r = issue_ref(conn, *issue_id)?;
            let mut stmt = conn.prepare_cached("SELECT 1 FROM users WHERE id = ?1")?;
            for u in user_ids {
                if !stmt.exists([u])? {
                    return invalid("unknown user");
                }
            }
            ("issues", *issue_id, format!("Edit assignees on {}", r.label))
        }
        Action::SubmitReview { pr_id, event, body_md, comments } => {
            let r = pr_ref(conn, *pr_id)?;
            if *event != ReviewEvent::Approve && body_md.trim().is_empty() && comments.is_empty() {
                return invalid("this review needs a comment");
            }
            if body_md.len() > MAX_BODY {
                return invalid("review is too long");
            }
            for c in comments {
                check_body(&c.body_md, "review comment")?;
                if c.path.is_empty() || c.line == 0 || c.start_line.is_some_and(|s| s == 0 || s >= c.line) {
                    return invalid("bad review comment position");
                }
            }
            ("issues", *pr_id, format!("Review on {}", r.label))
        }
        Action::ReplyThread { thread_id, body_md } => {
            check_body(body_md, "reply")?;
            ("review_threads", *thread_id, format!("Reply on {}", thread_ref(conn, *thread_id)?.label))
        }
        Action::ResolveThread { thread_id, resolved } => {
            let r = thread_ref(conn, *thread_id)?;
            let verb = if *resolved { "Resolve" } else { "Unresolve" };
            ("review_threads", *thread_id, format!("{verb} thread on {}", r.label))
        }
        Action::Merge { pr_id, method } => {
            let r = pr_ref(conn, *pr_id)?;
            if r.state != "open" {
                return invalid("only an open pull request can be merged");
            }
            let how = match method {
                MergeMethod::Merge => "Merge",
                MergeMethod::Squash => "Squash-merge",
                MergeMethod::Rebase => "Rebase-merge",
            };
            ("issues", *pr_id, format!("{how} {}", r.label))
        }
        Action::NotificationRead { id } | Action::NotificationDone { id } => {
            let n = notification_id(id)?;
            let title: String = conn
                .query_row("SELECT title FROM notifications WHERE id = ?1", [id], |r| r.get(0))
                .optional()?
                .ok_or_else(|| Error::Invalid("notification not found".into()))?;
            let verb = if matches!(action, Action::NotificationRead { .. }) { "Mark read" } else { "Mark done" };
            ("notifications", n, format!("{verb}: {title}"))
        }
    })
}

/// Record that `action_id` now owns `table.field` of `row`. If another
/// action already owns it, keep its `old_value_json` (the last value GitHub
/// confirmed) and drop that action when it is not in flight and owns no
/// other field (all field actions replace the whole value, so the older
/// one is redundant).
fn own(tx: &Transaction<'_>, action_id: i64, table: &str, row: i64, field: &str, old: Option<Value>) -> Result<()> {
    let prev: Option<i64> = tx
        .query_row(
            "SELECT action_id FROM pending_fields WHERE table_name = ?1 AND row_id = ?2 AND field = ?3",
            params![table, row, field],
            |r| r.get(0),
        )
        .optional()?;
    match prev {
        Some(prev) if prev == action_id => {}
        Some(prev) => {
            tx.execute(
                "UPDATE pending_fields SET action_id = ?4 WHERE table_name = ?1 AND row_id = ?2 AND field = ?3",
                params![table, row, field, action_id],
            )?;
            tx.execute(
                "DELETE FROM action_queue WHERE id = ?1 AND in_flight = 0 AND state = 'pending'
                   AND NOT EXISTS (SELECT 1 FROM pending_fields WHERE action_id = ?1)",
                [prev],
            )?;
        }
        None => {
            tx.execute(
                "INSERT INTO pending_fields (table_name, row_id, field, action_id, old_value_json) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![table, row, field, action_id, old.map(|v| v.to_string())],
            )?;
        }
    }
    Ok(())
}

fn local_id(tx: &Transaction<'_>, table: &str) -> rusqlite::Result<i64> {
    tx.query_row(&format!("SELECT min(COALESCE(min(id), 0), 0) - 1 FROM {table}"), [], |r| r.get(0))
}

fn viewer_id(tx: &Transaction<'_>) -> rusqlite::Result<Option<i64>> {
    let v: Option<String> = tx
        .query_row("SELECT value_json FROM settings WHERE key = 'viewer'", [], |r| r.get(0))
        .optional()?;
    let id = v.and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v["id"].as_i64());
    // Only use it when the user row exists (foreign key).
    match id {
        Some(id) if tx.prepare_cached("SELECT 1 FROM users WHERE id = ?1")?.exists([id])? => Ok(Some(id)),
        _ => Ok(None),
    }
}

fn link_ids(tx: &Transaction<'_>, table: &str, col: &str, issue: i64) -> rusqlite::Result<Vec<i64>> {
    tx.prepare(&format!("SELECT {col} FROM {table} WHERE issue_id = ?1 ORDER BY {col}"))?
        .query_map([issue], |r| r.get(0))?
        .collect()
}

fn set_links(tx: &Transaction<'_>, table: &str, col: &str, issue: i64, ids: &[i64]) -> rusqlite::Result<()> {
    tx.execute(&format!("DELETE FROM {table} WHERE issue_id = ?1"), [issue])?;
    let mut stmt = tx.prepare(&format!("INSERT OR IGNORE INTO {table} (issue_id, {col}) VALUES (?1, ?2)"))?;
    for id in ids {
        stmt.execute([issue, *id])?;
    }
    Ok(())
}

fn ids_of(v: &Value) -> Vec<i64> {
    v.as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default()
}

/// Minimal HTML for local rows that only have an HTML column (reviews).
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 7);
    out.push_str("<p>");
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("<br>"),
            c => out.push(c),
        }
    }
    out.push_str("</p>");
    out
}

/// Apply the action to local tables and record ownership in `pending_fields`.
fn apply(tx: &Transaction<'_>, id: ActionId, action: &Action, now: i64) -> Result<()> {
    match action {
        Action::Comment { issue_id, body_md } => {
            let local = local_id(tx, "comments")?;
            tx.execute(
                "INSERT INTO comments (id, issue_id, author_id, body_md, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![local, issue_id, viewer_id(tx)?, body_md, now],
            )?;
            own(tx, id, "comments", local, "row", None)?;
        }
        Action::SetState { issue_id, state } => set_state(tx, id, *issue_id, state.as_str())?,
        Action::Merge { pr_id, .. } => set_state(tx, id, *pr_id, "merged")?,
        Action::SetLabels { issue_id, label_ids } => {
            let old = link_ids(tx, "issue_labels", "label_id", *issue_id)?;
            set_links(tx, "issue_labels", "label_id", *issue_id, label_ids)?;
            own(tx, id, "issues", *issue_id, "labels", Some(json!(old)))?;
        }
        Action::SetAssignees { issue_id, user_ids } => {
            let old = link_ids(tx, "issue_assignees", "user_id", *issue_id)?;
            set_links(tx, "issue_assignees", "user_id", *issue_id, user_ids)?;
            own(tx, id, "issues", *issue_id, "assignees", Some(json!(old)))?;
        }
        Action::SubmitReview { pr_id, event, body_md, comments } => {
            let me = viewer_id(tx)?;
            let review = local_id(tx, "reviews")?;
            let state = match event {
                ReviewEvent::Approve => "APPROVED",
                ReviewEvent::RequestChanges => "CHANGES_REQUESTED",
                ReviewEvent::Comment => "COMMENTED",
            };
            let html = (!body_md.trim().is_empty()).then(|| escape_html(body_md));
            tx.execute(
                "INSERT INTO reviews (id, pr_id, author_id, state, body_html, submitted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![review, pr_id, me, state, html, now],
            )?;
            own(tx, id, "reviews", review, "row", None)?;
            let head: Option<String> = tx
                .query_row("SELECT head_oid FROM pr_details WHERE issue_id = ?1", [pr_id], |r| r.get(0))
                .optional()?;
            for c in comments {
                let thread = local_id(tx, "review_threads")?;
                tx.execute(
                    "INSERT INTO review_threads (id, node_id, pr_id, path, side, line, start_line, original_line, commit_oid)
                     VALUES (?1, '', ?2, ?3, ?4, ?5, ?6, ?5, ?7)",
                    params![thread, pr_id, c.path, side_str(c.side), c.line, c.start_line, head],
                )?;
                own(tx, id, "review_threads", thread, "row", None)?;
                let comment = local_id(tx, "review_comments")?;
                tx.execute(
                    "INSERT INTO review_comments (id, thread_id, review_id, author_id, body_md, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![comment, thread, review, me, c.body_md, now],
                )?;
                own(tx, id, "review_comments", comment, "row", None)?;
            }
        }
        Action::ReplyThread { thread_id, body_md } => {
            let local = local_id(tx, "review_comments")?;
            tx.execute(
                "INSERT INTO review_comments (id, thread_id, author_id, body_md, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![local, thread_id, viewer_id(tx)?, body_md, now],
            )?;
            own(tx, id, "review_comments", local, "row", None)?;
        }
        Action::ResolveThread { thread_id, resolved } => {
            let old: i64 = tx.query_row("SELECT is_resolved FROM review_threads WHERE id = ?1", [thread_id], |r| r.get(0))?;
            tx.execute("UPDATE review_threads SET is_resolved = ?1 WHERE id = ?2", params![resolved, thread_id])?;
            own(tx, id, "review_threads", *thread_id, "is_resolved", Some(json!(old)))?;
        }
        Action::NotificationRead { id: nid } | Action::NotificationDone { id: nid } => {
            let field = if matches!(action, Action::NotificationRead { .. }) { "unread" } else { "done" };
            let old: i64 = tx.query_row(&format!("SELECT {field} FROM notifications WHERE id = ?1"), [nid], |r| r.get(0))?;
            let new = i64::from(field == "done");
            tx.execute(&format!("UPDATE notifications SET {field} = ?1 WHERE id = ?2"), params![new, nid])?;
            own(tx, id, "notifications", notification_id(nid)?, field, Some(json!(old)))?;
        }
    }
    Ok(())
}

fn set_state(tx: &Transaction<'_>, id: ActionId, issue: i64, state: &str) -> Result<()> {
    let old: String = tx.query_row("SELECT state FROM issues WHERE id = ?1", [issue], |r| r.get(0))?;
    tx.execute("UPDATE issues SET state = ?1 WHERE id = ?2", params![state, issue])?;
    own(tx, id, "issues", issue, "state", Some(json!(old)))
}

fn side_str(side: fl_types::DiffSide) -> &'static str {
    match side {
        fl_types::DiffSide::Left => "LEFT",
        fl_types::DiffSide::Right => "RIGHT",
    }
}
