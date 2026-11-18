//! FTS5 search and operator parser (is:pr, author:, label:, repo:, ...). Owner: Phase 1 agent 5.
//!
//! Contract: sync (agent 1) and the fake generator call `index_issue` after
//! writing an issue or its comments, inside their write transaction. The
//! `search` command (agent 5) calls `search`.

mod query;
#[cfg(test)]
mod tests;

use fl_types::{IssueId, IssueRow, Page, PageReq, QueueKind, SearchHit};
use fl_db::queries::queue::Cond;
use fl_db::queries::{ISSUE_ROW_COLS, issue_row, page};
use query::{Filter, fts_terms, parse};
use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension, ToSql, params_from_iter};

/// When the terms together match more docs than this, results rank by recency, not bm25:
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

/// SQL condition (over aliases i, r, u) for one filter. `bind` stores a
/// value and returns its placeholder.
fn condition(f: &Filter, bind: &mut dyn FnMut(&str) -> String) -> String {
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
        Filter::Unread => "i.updated_at > COALESCE(i.last_viewed_at, 0)".into(),
    }
}

/// The `/` filter of a queue: `query` as one condition for
/// `fl_db::queries::queue`. Operators as in [`search`]; every free-text term
/// must hit the issue's title, body or a comment (the last one as a prefix),
/// and no negated term may. `None` when the query filters nothing.
///
/// A term matching few docs becomes the set of matching issue ids. Mapping
/// docs to issues costs a lookup per doc, so a broad term (more than
/// [`BROAD_DOCS`] docs) is checked per queue row instead: the doc ids are
/// cheap to collect, and a page stops after its rows.
///
/// `kind` picks what `is:unread` reads: the Inbox uses the notification's
/// unread flag (the row's unread mark), the other queues the issue's.
pub fn queue_filter(conn: &Connection, query: &str, kind: QueueKind) -> rusqlite::Result<Option<Cond>> {
    let parsed = parse(query);
    let (terms, neg) = fts_terms(&parsed.terms);
    let mut args: Vec<Value> = Vec::new();
    let mut conds: Vec<String> = parsed
        .filters
        .iter()
        .map(|(f, neg)| {
            let c = match f {
                Filter::Unread if kind == QueueKind::Inbox => "n.unread = 1".to_owned(),
                f => condition(f, &mut |v| {
                    args.push(Value::Text(v.to_owned()));
                    "?".to_owned()
                }),
            };
            if *neg { format!("NOT ({c})") } else { c }
        })
        .collect();
    let positive = terms.into_iter().map(|t| (t, false));
    for (t, negated) in positive.chain(neg.map(|n| (n, true))) {
        let docs: i64 = conn
            .prepare_cached("SELECT count(*) FROM (SELECT 1 FROM search_fts WHERE search_fts MATCH ?1 LIMIT 2001)")?
            .query_row([&t], |r| r.get(0))?;
        let hit = if docs > BROAD_DOCS {
            "EXISTS (SELECT 1 FROM search_docs d WHERE d.issue_id = i.id                AND d.id IN (SELECT rowid FROM search_fts WHERE search_fts MATCH ?))"
        } else {
            "i.id IN (SELECT d.issue_id FROM search_fts JOIN search_docs d ON d.id = search_fts.rowid                WHERE search_fts MATCH ?)"
        };
        conds.push(if negated { format!("NOT {hit}") } else { hit.to_owned() });
        args.push(Value::Text(t));
    }
    // Text that is neither an operator nor searchable (`*`, `-`) still
    // filters: it matches nothing, like a search for it would.
    if conds.is_empty() && !parsed.terms.is_empty() {
        conds.push("0".to_owned());
    }
    Ok((!conds.is_empty()).then(|| Cond { sql: format!("({})", conds.join(" AND ")), args }))
}

const SNIPPET_SQL: &str = "SELECT snippet(search_fts, -1, char(57344), char(57345), '...', 16)
     FROM search_fts WHERE search_fts MATCH ?1 AND rowid = ?2";

/// Run a user query (free text plus operators). An issue matches when every
/// free-text term hits at least one of its docs (title, body, any comment).
/// Invalid operators are treated as text, never as an error.
pub fn search(conn: &Connection, query: &str, req: PageReq) -> rusqlite::Result<Page<SearchHit>> {
    let req = req.clamped();
    let parsed = parse(query);
    let (terms, neg) = fts_terms(&parsed.terms);
    // Params: filter values first (?1..), then one MATCH string per term, the
    // negated-terms MATCH string, limit and offset.
    let mut values: Vec<String> = Vec::new();
    let mut wheres: Vec<String> = parsed
        .filters
        .iter()
        .map(|(f, neg)| {
            let c = condition(f, &mut |v| {
                values.push(v.to_owned());
                format!("?{}", values.len())
            });
            if *neg { format!("NOT ({c})") } else { c }
        })
        .collect();
    let mut args: Vec<Box<dyn ToSql>> = values.into_iter().map(|v| Box::new(v) as Box<dyn ToSql>).collect();
    let first_term = args.len() + 1;
    args.extend(terms.iter().map(|t| Box::new(t.clone()) as Box<dyn ToSql>));
    if let Some(n) = &neg {
        // Negated terms exclude the whole issue, whichever doc they hit.
        args.push(Box::new(n.clone()));
        wheres.push(format!(
            "i.id NOT IN (SELECT d.issue_id FROM search_fts JOIN search_docs d ON d.id = search_fts.rowid
               WHERE search_fts MATCH ?{})",
            args.len()
        ));
    }
    let where_sql = if wheres.is_empty() { "1".to_owned() } else { wheres.join(" AND ") };
    let limit_at = args.len() + 1;
    args.push(Box::new(req.limit + 1));
    args.push(Box::new(req.offset));

    // The narrow inner query joins only what its filters read.
    let uses = |f: fn(&Filter) -> bool| parsed.filters.iter().any(|(x, _)| f(x));
    let inner_joins = format!(
        "{} {}",
        if uses(|f| matches!(f, Filter::Repo(..))) { "JOIN repos r ON r.id = i.repo_id" } else { "" },
        if uses(|f| matches!(f, Filter::Author(_))) { "LEFT JOIN users u ON u.id = i.author_id" } else { "" },
    );

    // Docs matched by the terms together; capped counts keep this cheap.
    let mut matched = 0;
    for t in &terms {
        matched += conn
            .prepare_cached("SELECT count(*) FROM (SELECT 1 FROM search_fts WHERE search_fts MATCH ?1 LIMIT 2001)")?
            .query_row([t], |r| r.get::<_, i64>(0))?;
    }
    let broad = matched > BROAD_DOCS;
    // Highlights every term in the snippet of a doc.
    let any_term = terms.join(" OR ");

    // (row, search_docs id of the best doc, its comment_id)
    let rows: Vec<(IssueRow, i64, i64)> = if !terms.is_empty() && !broad {
        // One MATCH per term. `pk` holds the best score of each term per
        // issue; the issue matches when all terms are present. The shown doc
        // is the one matching most terms, then the best scored. Pick the page
        // of issues on narrow columns first, then build the wide rows.
        let hits = (0..terms.len())
            .map(|k| {
                format!(
                    "SELECT rowid, {k} AS k, bm25(search_fts, 10.0, 1.0) AS s
                     FROM search_fts WHERE search_fts MATCH ?{}",
                    first_term + k
                )
            })
            .collect::<Vec<_>>()
            .join(" UNION ALL ");
        let sql = format!(
            "WITH ux AS MATERIALIZED (
               SELECT d.issue_id AS iid, d.id AS did, d.comment_id AS cid, h.k, h.s
               FROM ({hits}) h JOIN search_docs d ON d.id = h.rowid),
             x AS MATERIALIZED (
               SELECT iid, did, cid, count(*) AS nk, sum(s) AS sc FROM ux GROUP BY did),
             pk AS MATERIALIZED (SELECT iid, k, MIN(s) AS s FROM ux GROUP BY iid, k),
             best AS (
               SELECT i.id AS iid, SUM(pk.s) AS sc
               FROM pk
               JOIN issues i ON i.id = pk.iid
               {inner_joins}
               WHERE {where_sql}
               GROUP BY i.id
               HAVING count(*) = {}
               ORDER BY sc, i.updated_at DESC
               LIMIT ?{limit_at} OFFSET ?{})
             SELECT {ISSUE_ROW_COLS},
               (SELECT did FROM x WHERE x.iid = best.iid ORDER BY nk DESC, sc LIMIT 1),
               (SELECT cid FROM x WHERE x.iid = best.iid ORDER BY nk DESC, sc LIMIT 1)
             FROM best
             JOIN issues i ON i.id = best.iid
             JOIN repos r ON r.id = i.repo_id
             LEFT JOIN users u ON u.id = i.author_id
             ORDER BY best.sc, i.updated_at DESC",
            terms.len(),
            limit_at + 1,
        );
        conn.prepare_cached(&sql)?
            .query_map(params_from_iter(args.iter()), |r| Ok((issue_row(r)?, r.get(11)?, r.get(12)?)))?
            .collect::<rusqlite::Result<_>>()?
    } else {
        // Operators only, or a broad text match: newest first, straight off
        // the `issues_updated` index, stopping after one page.
        let text: String = (0..terms.len())
            .map(|k| {
                format!(
                    "AND i.id IN (SELECT d.issue_id FROM search_fts
                       JOIN search_docs d ON d.id = search_fts.rowid WHERE search_fts MATCH ?{})",
                    first_term + k
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        let sql = format!(
            "SELECT {ISSUE_ROW_COLS}, 0, 0
             FROM issues i INDEXED BY issues_updated
             JOIN repos r ON r.id = i.repo_id
             LEFT JOIN users u ON u.id = i.author_id
             WHERE {where_sql} {text}
             ORDER BY i.updated_at DESC
             LIMIT ?{limit_at} OFFSET ?{}",
            limit_at + 1
        );
        conn.prepare_cached(&sql)?
            .query_map(params_from_iter(args.iter()), |r| Ok((issue_row(r)?, r.get(11)?, r.get(12)?)))?
            .collect::<rusqlite::Result<_>>()?
    };

    let mut hits = Vec::with_capacity(rows.len());
    for (row, doc_id, comment_id) in rows {
        let (snippet, comment_id) = if terms.is_empty() {
            (
                conn.prepare_cached("SELECT substr(COALESCE(body_md, ''), 1, 160) FROM issues WHERE id = ?1")?
                    .query_row([row.id], |r| r.get(0))?,
                0,
            )
        } else if hits.len() >= req.limit as usize {
            // The look-ahead row is dropped by `page`; skip its snippet.
            (String::new(), comment_id)
        } else if doc_id != 0 {
            (conn.prepare_cached(SNIPPET_SQL)?.query_row((&any_term, doc_id), |r| r.get(0))?, comment_id)
        } else {
            // Broad match: take this issue's first doc that matches any term.
            // Rowid lookups are cheap; a `d.issue_id` filter would rerun the
            // whole match.
            let docs = conn
                .prepare_cached("SELECT id, comment_id FROM search_docs WHERE issue_id = ?1 ORDER BY comment_id")?
                .query_map([row.id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut found = (String::new(), 0);
            for (id, cid) in docs {
                let s: Option<String> =
                    conn.prepare_cached(SNIPPET_SQL)?.query_row((&any_term, id), |r| r.get(0)).optional()?;
                if let Some(s) = s {
                    found = (s, cid);
                    break;
                }
            }
            found
        };
        hits.push(SearchHit {
            row,
            snippet,
            comment_id: (comment_id != 0).then_some(comment_id),
        });
    }
    Ok(page(hits, req))
}
