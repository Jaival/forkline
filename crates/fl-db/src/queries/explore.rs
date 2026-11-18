//! Explore: repos starred lately by the people the user follows.

use fl_types::{ExploreRow, Page, PageReq};
use rusqlite::Connection;

use super::page;

/// Logins shown per row.
const STARRED_BY: u32 = 3;

/// One row per repo, newest star first. `explore_stars` rows go with their
/// `following` row (cascade), so every star here is from a followed user.
/// Reads every `explore_stars` row (covering index) and sorts the groups:
/// the table holds only recent stars of followed users (fake data: 500).
pub const FEED_SQL: &str = "SELECT r.id, r.owner, r.name, r.description, r.language, r.stars, r.starred, s.last, s.n
     FROM (SELECT repo_id, max(starred_at) AS last, count(*) AS n FROM explore_stars GROUP BY repo_id) s
     JOIN repos r ON r.id = s.repo_id
     ORDER BY s.last DESC, r.id LIMIT ?1 OFFSET ?2";

/// The newest stargazers of one repo.
pub const STARRED_BY_SQL: &str = "SELECT u.login FROM explore_stars e JOIN users u ON u.id = e.user_id
     WHERE e.repo_id = ?1 ORDER BY e.starred_at DESC LIMIT ?2";

pub fn feed(conn: &Connection, req: PageReq) -> rusqlite::Result<Page<ExploreRow>> {
    let req = req.clamped();
    let mut rows = conn
        .prepare_cached(FEED_SQL)?
        .query_map((req.limit + 1, req.offset), |r| {
            Ok(ExploreRow {
                repo_id: r.get(0)?,
                owner: r.get(1)?,
                name: r.get(2)?,
                description: r.get(3)?,
                language: r.get(4)?,
                stars: r.get(5)?,
                starred: r.get(6)?,
                starred_by: Vec::new(),
                last_starred_at: r.get(7)?,
                starred_by_count: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut by = conn.prepare_cached(STARRED_BY_SQL)?;
    for row in rows.iter_mut().take(req.limit as usize) {
        row.starred_by = by.query_map((row.repo_id, STARRED_BY), |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    }
    Ok(page(rows, req))
}
