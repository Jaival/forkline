//! PR view queries (details, reviews, threads, checks). Owner: agent 4.

use fl_types::{
    DiffSide, IssueId, Label, PAGE_MAX, Page, PageReq, PrCheck, PrComment, PrCommit, PrDetail,
    PrReview, ReviewThread, ReviewThreadComment,
};
use rusqlite::{Connection, OptionalExtension};

use super::{ISSUE_ROW_COLS, issue_row, page};

/// What `fl-git` needs to diff a PR.
pub struct DiffTarget {
    pub owner: String,
    pub name: String,
    pub base_oid: String,
    pub head_oid: String,
}

pub fn diff_target(conn: &Connection, pr_id: IssueId) -> rusqlite::Result<Option<DiffTarget>> {
    conn.prepare_cached(
        "SELECT r.owner, r.name, d.base_oid, d.head_oid FROM pr_details d
         JOIN issues i ON i.id = d.issue_id JOIN repos r ON r.id = i.repo_id
         WHERE d.issue_id = ?1",
    )?
    .query_row([pr_id], |r| {
        Ok(DiffTarget {
            owner: r.get(0)?,
            name: r.get(1)?,
            base_oid: r.get(2)?,
            head_oid: r.get(3)?,
        })
    })
    .optional()
}

pub fn get(conn: &Connection, id: IssueId) -> rusqlite::Result<Option<PrDetail>> {
    let sql = format!(
        "SELECT {ISSUE_ROW_COLS}, i.body_html, i.body_md, i.created_at,
                d.base_ref, d.head_ref, d.base_oid, d.head_oid, d.is_draft, d.mergeable,
                d.merge_state, d.review_decision, d.check_state, d.additions, d.deletions,
                d.changed_files, i.repo_id
         FROM issues i JOIN pr_details d ON d.issue_id = i.id
         JOIN repos r ON r.id = i.repo_id LEFT JOIN users u ON u.id = i.author_id
         WHERE i.id = ?1"
    );
    let found = conn
        .prepare_cached(&sql)?
        .query_row([id], |r| {
            let detail = PrDetail {
                row: issue_row(r)?,
                body_html: r.get(11)?,
                body_md: r.get(12)?,
                created_at: r.get(13)?,
                base_ref: r.get(14)?,
                head_ref: r.get(15)?,
                base_oid: r.get(16)?,
                head_oid: r.get(17)?,
                is_draft: r.get(18)?,
                mergeable: r.get(19)?,
                merge_state: r.get(20)?,
                review_decision: r.get(21)?,
                check_state: r.get(22)?,
                additions: r.get(23)?,
                deletions: r.get(24)?,
                changed_files: r.get(25)?,
                labels: Vec::new(),
                review_requests: Vec::new(),
                pending_fields: Vec::new(),
                comments: Vec::new(),
                reviews: Vec::new(),
                checks: Vec::new(),
                commits: Vec::new(),
            };
            Ok((detail, r.get::<_, i64>(26)?))
        })
        .optional()?;
    let Some((mut d, repo_id)) = found else {
        return Ok(None);
    };

    d.labels = conn
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
    d.review_requests = conn
        .prepare_cached("SELECT reviewer_login FROM pr_review_requests WHERE pr_id = ?1")?
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    d.pending_fields = conn
        .prepare_cached(
            "SELECT field FROM pending_fields WHERE table_name = 'issues' AND row_id = ?1",
        )?
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    d.comments = conn
        .prepare_cached(
            "SELECT c.id, COALESCE(u.login, 'ghost'), c.body_html, c.body_md, c.created_at
             FROM comments c LEFT JOIN users u ON u.id = c.author_id
             WHERE c.issue_id = ?1 ORDER BY c.created_at LIMIT ?2",
        )?
        .query_map((id, PAGE_MAX), |r| {
            Ok(PrComment {
                id: r.get(0)?,
                author: r.get(1)?,
                body_html: r.get(2)?,
                body_md: r.get(3)?,
                created_at: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    d.reviews = conn
        .prepare_cached(
            "SELECT v.id, COALESCE(u.login, 'ghost'), v.state, v.body_html, v.body_md, v.submitted_at
             FROM reviews v LEFT JOIN users u ON u.id = v.author_id
             WHERE v.pr_id = ?1 ORDER BY v.submitted_at LIMIT ?2",
        )?
        .query_map((id, PAGE_MAX), |r| {
            Ok(PrReview {
                id: r.get(0)?,
                author: r.get(1)?,
                state: r.get(2)?,
                body_html: r.get(3)?,
                body_md: r.get(4)?,
                submitted_at: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    d.checks = conn
        .prepare_cached(
            "SELECT id, name, status, conclusion, url, COALESCE(completed_at, started_at) FROM checks
             WHERE repo_id = ?1 AND commit_oid = ?2 ORDER BY name LIMIT ?3",
        )?
        .query_map((repo_id, &d.head_oid, PAGE_MAX), |r| {
            Ok(PrCheck {
                id: r.get(0)?,
                name: r.get(1)?,
                status: r.get(2)?,
                conclusion: r.get(3)?,
                url: r.get(4)?,
                at: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    d.commits = conn
        .prepare_cached(
            "SELECT p.oid, c.headline, c.author_login, c.authored_at
             FROM pr_commits p LEFT JOIN commits c ON c.repo_id = ?2 AND c.oid = p.oid
             WHERE p.pr_id = ?1 ORDER BY p.position LIMIT ?3",
        )?
        .query_map((id, repo_id, PAGE_MAX), |r| {
            Ok(PrCommit {
                oid: r.get(0)?,
                headline: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                author: r.get(2)?,
                authored_at: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Some(d))
}

/// Review threads of a PR ordered by path, with their comments.
pub fn threads(
    conn: &Connection,
    pr_id: IssueId,
    req: PageReq,
) -> rusqlite::Result<Page<ReviewThread>> {
    let mut items: Vec<ReviewThread> = conn
        .prepare_cached(
            "SELECT t.id, t.path, t.side, t.line, t.start_line, t.original_line, t.is_resolved,
                    t.is_outdated,
                    EXISTS (SELECT 1 FROM pending_fields p
                            WHERE p.table_name = 'review_threads' AND p.row_id = t.id)
             FROM review_threads t WHERE t.pr_id = ?1 ORDER BY t.path, t.id LIMIT ?2 OFFSET ?3",
        )?
        .query_map((pr_id, req.limit + 1, req.offset), |r| {
            let side: String = r.get(2)?;
            Ok(ReviewThread {
                id: r.get(0)?,
                path: r.get(1)?,
                side: if side == "LEFT" { DiffSide::Left } else { DiffSide::Right },
                line: r.get(3)?,
                start_line: r.get(4)?,
                original_line: r.get(5)?,
                is_resolved: r.get(6)?,
                is_outdated: r.get(7)?,
                pending: r.get(8)?,
                comments: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut stmt = conn.prepare_cached(
        "SELECT c.id, COALESCE(u.login, 'ghost'), c.body_html, c.body_md, c.created_at
         FROM review_comments c LEFT JOIN users u ON u.id = c.author_id
         WHERE c.thread_id = ?1 ORDER BY c.created_at, c.id",
    )?;
    for t in items.iter_mut().take(req.limit as usize) {
        t.comments = stmt
            .query_map([t.id], |r| {
                Ok(ReviewThreadComment {
                    id: r.get(0)?,
                    author: r.get(1)?,
                    body_html: r.get(2)?,
                    body_md: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
    }
    Ok(page(items, req))
}

/// Paths marked viewed at the PR's current head. Rows from an older head are ignored.
pub fn viewed_paths(conn: &Connection, pr_id: IssueId) -> rusqlite::Result<Vec<String>> {
    conn.prepare_cached(
        "SELECT v.path FROM pr_viewed_files v JOIN pr_details d ON d.issue_id = v.pr_id
         WHERE v.pr_id = ?1 AND v.head_oid = d.head_oid",
    )?
    .query_map([pr_id], |r| r.get(0))?
    .collect()
}

pub fn set_viewed(
    conn: &Connection,
    pr_id: IssueId,
    path: &str,
    viewed: bool,
) -> rusqlite::Result<()> {
    if viewed {
        conn.prepare_cached(
            "INSERT OR REPLACE INTO pr_viewed_files(pr_id, path, head_oid)
             SELECT issue_id, ?2, head_oid FROM pr_details WHERE issue_id = ?1",
        )?
        .execute((pr_id, path))?;
    } else {
        conn.prepare_cached("DELETE FROM pr_viewed_files WHERE pr_id = ?1 AND path = ?2")?
            .execute((pr_id, path))?;
    }
    Ok(())
}

/// Opening a PR clears its unread dot (GitHub state is not changed).
pub fn mark_viewed(conn: &Connection, id: IssueId, now: i64) -> rusqlite::Result<()> {
    conn.prepare_cached("UPDATE issues SET last_viewed_at = ?2 WHERE id = ?1")?
        .execute((id, now))?;
    Ok(())
}
