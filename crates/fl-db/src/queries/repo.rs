//! Repository queries. Owner: agent 2 (sidebar list added in Phase 0).

use fl_types::RepoSummary;
use rusqlite::Connection;

/// Pinned repositories with open PR / issue counts, for the sidebar.
pub fn pinned(conn: &Connection) -> rusqlite::Result<Vec<RepoSummary>> {
    conn.prepare_cached(
        "SELECT r.id, r.owner, r.name, r.pinned,
           (SELECT count(*) FROM issues i WHERE i.repo_id = r.id AND i.is_pr = 1 AND i.state = 'open'),
           (SELECT count(*) FROM issues i WHERE i.repo_id = r.id AND i.is_pr = 0 AND i.state = 'open')
         FROM repos r WHERE r.pinned = 1 ORDER BY r.owner, r.name",
    )?
    .query_map([], |r| {
        Ok(RepoSummary {
            id: r.get(0)?,
            owner: r.get(1)?,
            name: r.get(2)?,
            pinned: r.get(3)?,
            open_prs: r.get(4)?,
            open_issues: r.get(5)?,
        })
    })?
    .collect()
}
