//! FTS5 search and operator parser (is:pr, author:, label:, repo:, ...). Owner: Phase 1 agent 5.
//!
//! Contract: sync (agent 1) and the fake generator call `index_issue` after
//! writing an issue or its comments, inside their write transaction. The
//! `search` command (agent 5) calls `search`. Signatures are fixed; agent 5
//! replaces the bodies.

use fl_types::{IssueId, Page, PageReq, SearchHit};
use rusqlite::Connection;

/// Rebuild the `search_docs` / `search_fts` rows for one issue and its
/// comments from the `issues` and `comments` tables.
pub fn index_issue(conn: &Connection, issue_id: IssueId) -> rusqlite::Result<()> {
    let _ = (conn, issue_id);
    Ok(())
}

/// Run a user query (free text plus operators). Invalid operators are
/// treated as text, never as an error.
pub fn search(conn: &Connection, query: &str, page: PageReq) -> rusqlite::Result<Page<SearchHit>> {
    let _ = (conn, query, page);
    Ok(Page {
        items: Vec::new(),
        next: None,
    })
}
