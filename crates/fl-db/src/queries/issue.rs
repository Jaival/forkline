//! Issue detail queries. Owner: agent 3 (basic `get` added in Phase 0).

use fl_types::{
    CommentRow, EditOptions, IssueDetail, IssueId, IssueThread, Label, RepoId, TimelineEvent, User,
};
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

/// `number`, `title` and `state` (`open` / `closed` / `merged`): what a tab
/// shows for the item.
pub fn tab_label(conn: &Connection, id: IssueId) -> rusqlite::Result<Option<(i64, String, String, String)>> {
    conn.prepare_cached(
        "SELECT i.number, i.title, i.state, r.owner || '/' || r.name FROM issues i JOIN repos r ON r.id = i.repo_id
         WHERE i.id = ?1",
    )?
    .query_row([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
    .optional()
}

/// Opening an item clears its unread dot (GitHub state is not changed).
pub fn mark_viewed(conn: &Connection, id: IssueId, now: i64) -> rusqlite::Result<()> {
    conn.prepare_cached("UPDATE issues SET last_viewed_at = ?2 WHERE id = ?1")?
        .execute((id, now))?;
    Ok(())
}

/// Assignees of an issue or pull request, by login.
pub fn assignees(conn: &Connection, id: IssueId) -> rusqlite::Result<Vec<User>> {
    conn.prepare_cached(
        "SELECT u.id, u.login FROM issue_assignees a JOIN users u ON u.id = a.user_id
         WHERE a.issue_id = ?1 ORDER BY u.login",
    )?
    .query_map([id], |r| {
        Ok(User {
            id: r.get(0)?,
            login: r.get(1)?,
        })
    })?
    .collect()
}

/// Conversation, timeline, assignees, linked items and pending fields.
pub fn thread(conn: &Connection, id: IssueId) -> rusqlite::Result<IssueThread> {
    let assignees = assignees(conn, id)?;
    let comments = conn
        .prepare_cached(
            "SELECT c.id, COALESCE(u.login, 'ghost'), c.body_html, c.body_md, c.created_at
             FROM comments c LEFT JOIN users u ON u.id = c.author_id
             WHERE c.issue_id = ?1 ORDER BY c.created_at LIMIT 100",
        )?
        .query_map([id], |r| {
            Ok(CommentRow {
                id: r.get(0)?,
                author: r.get(1)?,
                body_html: r.get(2)?,
                body_md: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let timeline = conn
        .prepare_cached(
            "SELECT t.id, t.kind, COALESCE(u.login, 'ghost'), t.created_at, t.payload_json
             FROM timeline_events t LEFT JOIN users u ON u.id = t.actor_id
             WHERE t.issue_id = ?1 ORDER BY t.created_at LIMIT 100",
        )?
        .query_map([id], |r| {
            Ok(TimelineEvent {
                id: r.get(0)?,
                kind: r.get(1)?,
                actor: r.get(2)?,
                created_at: r.get(3)?,
                payload_json: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    // PRs linked to this issue, then issues linked to this PR.
    let mut linked = Vec::new();
    for (this, other) in [("issue_id", "pr_id"), ("pr_id", "issue_id")] {
        let sql = format!(
            "SELECT {ISSUE_ROW_COLS} FROM issue_links l JOIN issues i ON i.id = l.{other}
             JOIN repos r ON r.id = i.repo_id LEFT JOIN users u ON u.id = i.author_id
             WHERE l.{this} = ?1 ORDER BY i.number"
        );
        linked.extend(
            conn.prepare_cached(&sql)?
                .query_map([id], issue_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        );
    }
    let pending_fields = conn
        .prepare_cached(
            "SELECT field FROM pending_fields WHERE table_name = 'issues' AND row_id = ?1",
        )?
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(IssueThread {
        assignees,
        comments,
        timeline,
        linked,
        pending_fields,
    })
}

/// Labels of the issue's repo and known users, for the label / assignee editors.
pub fn edit_options(conn: &Connection, repo_id: RepoId) -> rusqlite::Result<EditOptions> {
    let labels = conn
        .prepare_cached("SELECT id, name, color FROM labels WHERE repo_id = ?1 ORDER BY name")?
        .query_map([repo_id], |r| {
            Ok(Label {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let users = conn
        .prepare_cached("SELECT id, login FROM users ORDER BY login LIMIT 100")?
        .query_map([], |r| {
            Ok(User {
                id: r.get(0)?,
                login: r.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(EditOptions { labels, users })
}
