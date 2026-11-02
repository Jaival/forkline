//! FTS5 search and operator parser (is:pr, author:, label:, repo:, ...). Owner: Phase 1 agent 5.
//!
//! Contract: sync (agent 1) and the fake generator call `index_issue` after
//! writing an issue or its comments, inside their write transaction. The
//! `search` command (agent 5) calls `search`.

mod query;
#[cfg(test)]
mod tests;

use fl_types::{IssueId, IssueRow, Page, PageReq, SearchHit};
use fl_db::queries::{ISSUE_ROW_COLS, issue_row, page};
use query::{Filter, fts_match, parse};
use rusqlite::{Connection, OptionalExtension, ToSql, params_from_iter};

/// A match set larger than this is ranked by recency instead of bm25:
/// scoring costs about 1.5 us per doc, and with that many hits every term is
/// common, so bm25 separates little.
const BROAD_DOCS: i64 = 2000;

/// Rebuild the `search_docs` / `search_fts` rows for one issue and its
/// comments from the `issues` and `comments` tables.
pub fn index_issue(conn: &Connection, issue_id: IssueId) -> rusqlite::Result<()> {
    // The trigger on search_docs removes the matching FTS rows.
    conn.prepare_cached("DELETE FROM search_docs WHERE issue_id = ?1")?
        .execute([issue_id])?;
    conn.prepare_cached(
        "INSERT INTO search_docs(issue_id, comment_id) SELECT id, 0 FROM issues WHERE id = ?1",
    )?
    .execute([issue_id])?;
    conn.prepare_cached(
        "INSERT INTO search_fts(rowid, title, body)
         SELECT d.id, i.title, COALESCE(i.body_md, '')
         FROM search_docs d JOIN issues i ON i.id = d.issue_id
         WHERE d.issue_id = ?1 AND d.comment_id = 0",
    )?
    .execute([issue_id])?;
    conn.prepare_cached(
        "INSERT INTO search_docs(issue_id, comment_id)
         SELECT issue_id, id FROM comments
         WHERE issue_id = ?1 AND COALESCE(body_md, '') <> ''",
    )?
    .execute([issue_id])?;
    conn.prepare_cached(
        "INSERT INTO search_fts(rowid, title, body)
         SELECT d.id, '', c.body_md
         FROM search_docs d JOIN comments c ON c.id = d.comment_id
         WHERE d.issue_id = ?1 AND d.comment_id <> 0",
    )?
    .execute([issue_id])?;
    Ok(())
}

/// SQL condition (over aliases i, r) and bound values for one filter.
fn condition(f: &Filter, params: &mut Vec<String>) -> String {
    let mut bind = |v: &str| {
        params.push(v.to_owned());
        format!("?{}", params.len() + 1) // ?1 is reserved for the MATCH string
    };
    match f {
        Filter::IsPr => "i.is_pr = 1".into(),
        Filter::IsIssue => "i.is_pr = 0".into(),
        Filter::State(s) => format!("i.state = '{s}'"),
        Filter::Author(a) => format!("u.login = {} COLLATE NOCASE", bind(a)),
        Filter::Label(l) => format!(
            "EXISTS (SELECT 1 FROM issue_labels il JOIN labels l ON l.id = il.label_id
              WHERE il.issue_id = i.id AND l.name = {} COLLATE NOCASE)",
            bind(l)
        ),
        Filter::Repo(o, n) => format!(
            "(r.owner = {} COLLATE NOCASE AND r.name = {} COLLATE NOCASE)",
            bind(o),
            bind(n)
        ),
        Filter::Assignee(a) => format!(
            "EXISTS (SELECT 1 FROM issue_assignees ia JOIN users au ON au.id = ia.user_id
              WHERE ia.issue_id = i.id AND au.login = {} COLLATE NOCASE)",
            bind(a)
        ),
    }
}

/// Run a user query (free text plus operators). Invalid operators are
/// treated as text, never as an error.
pub fn search(conn: &Connection, query: &str, req: PageReq) -> rusqlite::Result<Page<SearchHit>> {
    let req = req.clamped();
    let parsed = parse(query);
    let fts = fts_match(&parsed.terms);
    // Params: ?1 = MATCH string (unused without text), then filter values.
    let mut values: Vec<String> = Vec::new();
    let wheres: Vec<String> = parsed
        .filters
        .iter()
        .map(|(f, neg)| {
            let c = condition(f, &mut values);
            if *neg { format!("NOT ({c})") } else { c }
        })
        .collect();
    let where_sql = if wheres.is_empty() { "1".to_owned() } else { wheres.join(" AND ") };

    // The narrow inner query joins only what its filters read.
    let uses = |f: fn(&Filter) -> bool| parsed.filters.iter().any(|(x, _)| f(x));
    let inner_joins = format!(
        "{} {}",
        if uses(|f| matches!(f, Filter::Repo(..))) { "JOIN repos r ON r.id = i.repo_id" } else { "" },
        if uses(|f| matches!(f, Filter::Author(_))) { "LEFT JOIN users u ON u.id = i.author_id" } else { "" },
    );

    let mut args: Vec<Box<dyn ToSql>> = vec![Box::new(fts.clone().unwrap_or_default())];
    args.extend(values.into_iter().map(|v| Box::new(v) as Box<dyn ToSql>));
    let n = args.len();
    args.push(Box::new(req.limit + 1));
    args.push(Box::new(req.offset));

    let broad = match &fts {
        Some(m) => {
            let n: i64 = conn
                .prepare_cached("SELECT count(*) FROM (SELECT 1 FROM search_fts WHERE search_fts MATCH ?1 LIMIT 2001)")?
                .query_row([m], |r| r.get(0))?;
            n > BROAD_DOCS
        }
        None => false,
    };

    // (row, search_docs id of the best doc, its comment_id)
    let rows: Vec<(IssueRow, i64, i64)> = if fts.is_some() && !broad {
        // Best-ranked doc per issue: bare columns come from the MIN() row.
        // Pick the page of (issue, best doc) on narrow columns first, then
        // build the wide rows for only those issues.
        let sql = format!(
            "WITH h AS MATERIALIZED (
               SELECT rowid, bm25(search_fts, 10.0, 1.0) AS score
               FROM search_fts WHERE search_fts MATCH ?1),
             best AS (
               SELECT i.id AS iid, d.id AS did, d.comment_id AS cid, MIN(h.score) AS sc
               FROM h
               JOIN search_docs d ON d.id = h.rowid
               JOIN issues i ON i.id = d.issue_id
               {inner_joins}
               WHERE {where_sql}
               GROUP BY i.id
               ORDER BY sc, i.updated_at DESC
               LIMIT ?{} OFFSET ?{})
             SELECT {ISSUE_ROW_COLS}, best.did, best.cid
             FROM best
             JOIN issues i ON i.id = best.iid
             JOIN repos r ON r.id = i.repo_id
             LEFT JOIN users u ON u.id = i.author_id
             ORDER BY best.sc, i.updated_at DESC",
            n + 1,
            n + 2
        );
        conn.prepare_cached(&sql)?
            .query_map(params_from_iter(args.iter()), |r| Ok((issue_row(r)?, r.get(11)?, r.get(12)?)))?
            .collect::<rusqlite::Result<_>>()?
    } else {
        // Operators only, or a broad text match: newest first, straight off
        // the `issues_updated` index, stopping after one page.
        let (with, text) = if fts.is_some() {
            (
                "WITH m AS MATERIALIZED (SELECT d.issue_id AS iid FROM search_fts
                   JOIN search_docs d ON d.id = search_fts.rowid WHERE search_fts MATCH ?1)",
                "AND i.id IN (SELECT iid FROM m)",
            )
        } else {
            ("", "")
        };
        let sql = format!(
            "{with} SELECT {ISSUE_ROW_COLS}, 0, 0
             FROM issues i INDEXED BY issues_updated
             JOIN repos r ON r.id = i.repo_id
             LEFT JOIN users u ON u.id = i.author_id
             WHERE {where_sql} {text}
             ORDER BY i.updated_at DESC
             LIMIT ?{} OFFSET ?{}",
            n + 1,
            n + 2
        );
        // Without text, ?1 is unused but still bound.
        conn.prepare_cached(&sql)?
            .query_map(params_from_iter(args.iter()), |r| Ok((issue_row(r)?, r.get(11)?, r.get(12)?)))?
            .collect::<rusqlite::Result<_>>()?
    };

    let mut hits = Vec::with_capacity(rows.len());
    for (row, doc_id, comment_id) in rows {
        let (snippet, comment_id) = match &fts {
            // The look-ahead row is dropped by `page`; skip its snippet.
            Some(_) if hits.len() >= req.limit as usize => (String::new(), comment_id),
            Some(m) if doc_id != 0 => (
                conn.prepare_cached(
                    "SELECT snippet(search_fts, -1, char(57344), char(57345), '...', 16)
                     FROM search_fts WHERE search_fts MATCH ?1 AND rowid = ?2",
                )?
                .query_row((m, doc_id), |r| r.get(0))?,
                comment_id,
            ),
            // Broad match: find this issue's best doc now. Rowid lookups
            // are cheap; a `d.issue_id` filter would rerun the whole match.
            Some(m) => {
                let docs = conn
                    .prepare_cached("SELECT id, comment_id FROM search_docs WHERE issue_id = ?1 ORDER BY comment_id")?
                    .query_map([row.id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let mut found = (String::new(), 0);
                for (id, cid) in docs {
                    let s: Option<String> = conn
                        .prepare_cached(
                            "SELECT snippet(search_fts, -1, char(57344), char(57345), '...', 16)
                             FROM search_fts WHERE search_fts MATCH ?1 AND rowid = ?2",
                        )?
                        .query_row((m, id), |r| r.get(0))
                        .optional()?;
                    if let Some(s) = s {
                        found = (s, cid);
                        break;
                    }
                }
                found
            }
            None => (
                conn.prepare_cached("SELECT substr(COALESCE(body_md, ''), 1, 160) FROM issues WHERE id = ?1")?
                    .query_row([row.id], |r| r.get(0))?,
                0,
            ),
        };
        hits.push(SearchHit {
            row,
            snippet,
            comment_id: (comment_id != 0).then_some(comment_id),
        });
    }
    Ok(page(hits, req))
}
