//! Repository queries. Owner: agent 2.

use fl_types::{
    CommitRow, ContributorRow, LanguageShare, Page, PageReq, ReleaseRow, RepoFilter, RepoId,
    RepoOverview, RepoRow, RepoSummary, StarPoint,
};
use rusqlite::{Connection, OptionalExtension};

use super::page;

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

/// Escape `%`, `_` and `\` so user text matches literally in `LIKE ... ESCAPE '\'`.
fn like_pattern(q: &str) -> String {
    let mut s = String::with_capacity(q.len() + 2);
    s.push('%');
    for c in q.chars() {
        if matches!(c, '%' | '_' | '\\') {
            s.push('\\');
        }
        s.push(c);
    }
    s.push('%');
    s
}

/// Columns of `RepoRow`; alias `r` = repos.
pub const LIST_SQL_STARRED: &str = "SELECT r.id, r.owner, r.name, r.description, r.language, r.stars, r.pinned, r.starred,
       (SELECT count(*) FROM issues i WHERE i.repo_id = r.id AND i.is_pr = 1 AND i.state = 'open'),
       (SELECT count(*) FROM issues i WHERE i.repo_id = r.id AND i.is_pr = 0 AND i.state = 'open')
     FROM repos r
     WHERE r.starred = 1 AND (r.owner || '/' || r.name) LIKE ?1 ESCAPE '\\'
     ORDER BY r.owner, r.name LIMIT ?2 OFFSET ?3";

pub const LIST_SQL_ALL: &str = "SELECT r.id, r.owner, r.name, r.description, r.language, r.stars, r.pinned, r.starred,
       (SELECT count(*) FROM issues i WHERE i.repo_id = r.id AND i.is_pr = 1 AND i.state = 'open'),
       (SELECT count(*) FROM issues i WHERE i.repo_id = r.id AND i.is_pr = 0 AND i.state = 'open')
     FROM repos r
     WHERE (r.owner || '/' || r.name) LIKE ?1 ESCAPE '\\'
     ORDER BY r.owner, r.name LIMIT ?2 OFFSET ?3";

/// All known repos (or only starred ones), filtered by name, ordered by owner / name.
pub fn list(conn: &Connection, f: &RepoFilter, req: PageReq) -> rusqlite::Result<Page<RepoRow>> {
    let sql = if f.starred_only { LIST_SQL_STARRED } else { LIST_SQL_ALL };
    let items = conn
        .prepare_cached(sql)?
        .query_map((like_pattern(&f.query), req.limit + 1, req.offset), |r| {
            Ok(RepoRow {
                id: r.get(0)?,
                owner: r.get(1)?,
                name: r.get(2)?,
                description: r.get(3)?,
                language: r.get(4)?,
                stars: r.get(5)?,
                pinned: r.get(6)?,
                starred: r.get(7)?,
                open_prs: r.get(8)?,
                open_issues: r.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(page(items, req))
}

/// Local write. Returns false if the repo does not exist.
pub fn set_pinned(conn: &Connection, id: RepoId, pinned: bool) -> rusqlite::Result<bool> {
    Ok(conn
        .prepare_cached("UPDATE repos SET pinned = ?2 WHERE id = ?1")?
        .execute((id, pinned))?
        > 0)
}

const OVERVIEW_COMMITS: u32 = 10;
const OVERVIEW_RELEASES: u32 = 5;
const OVERVIEW_CONTRIBUTORS: u32 = 8;
const OVERVIEW_STAR_DAYS: u32 = 90;

pub fn overview(conn: &Connection, id: RepoId) -> rusqlite::Result<Option<RepoOverview>> {
    let head = conn
        .prepare_cached(
            "SELECT r.owner, r.name, r.description, r.default_branch, r.license, r.stars, r.forks,
               r.pinned, r.starred, r.languages_json,
               (SELECT count(*) FROM issues i WHERE i.repo_id = r.id AND i.is_pr = 1 AND i.state = 'open'),
               (SELECT count(*) FROM issues i WHERE i.repo_id = r.id AND i.is_pr = 0 AND i.state = 'open')
             FROM repos r WHERE r.id = ?1",
        )?
        .query_row([id], |r| {
            Ok((
                RepoOverview {
                    id,
                    owner: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    default_branch: r.get(3)?,
                    license: r.get(4)?,
                    stars: r.get(5)?,
                    forks: r.get(6)?,
                    pinned: r.get(7)?,
                    starred: r.get(8)?,
                    open_prs: r.get(10)?,
                    open_issues: r.get(11)?,
                    languages: vec![],
                    commits: vec![],
                    releases: vec![],
                    contributors: vec![],
                    star_history: vec![],
                },
                r.get::<_, Option<String>>(9)?,
            ))
        })
        .optional()?;
    let Some((mut o, languages_json)) = head else {
        return Ok(None);
    };
    // Written by sync / fake as [{"name":..,"bytes":..}]; bad JSON shows no bar.
    o.languages = languages_json
        .and_then(|j| serde_json::from_str::<Vec<LanguageShare>>(&j).ok())
        .unwrap_or_default();
    o.commits = conn
        .prepare_cached(
            "SELECT oid, headline, author_login, authored_at, check_state FROM commits
             WHERE repo_id = ?1 AND on_default_branch = 1 ORDER BY authored_at DESC LIMIT ?2",
        )?
        .query_map((id, OVERVIEW_COMMITS), |r| {
            Ok(CommitRow {
                oid: r.get(0)?,
                headline: r.get(1)?,
                author: r.get(2)?,
                authored_at: r.get(3)?,
                check_state: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    o.releases = conn
        .prepare_cached(
            "SELECT tag, name, published_at FROM releases WHERE repo_id = ?1
             ORDER BY published_at DESC LIMIT ?2",
        )?
        .query_map((id, OVERVIEW_RELEASES), |r| {
            Ok(ReleaseRow { tag: r.get(0)?, name: r.get(1)?, published_at: r.get(2)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    o.contributors = conn
        .prepare_cached(
            "SELECT u.login, c.contributions FROM repo_contributors c JOIN users u ON u.id = c.user_id
             WHERE c.repo_id = ?1 ORDER BY c.contributions DESC LIMIT ?2",
        )?
        .query_map((id, OVERVIEW_CONTRIBUTORS), |r| {
            Ok(ContributorRow { login: r.get(0)?, contributions: r.get(1)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut history = conn
        .prepare_cached(
            "SELECT day, stars FROM star_history WHERE repo_id = ?1 ORDER BY day DESC LIMIT ?2",
        )?
        .query_map((id, OVERVIEW_STAR_DAYS), |r| {
            Ok(StarPoint { day: r.get(0)?, stars: r.get(1)? })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    history.reverse();
    o.star_history = history;
    Ok(Some(o))
}
