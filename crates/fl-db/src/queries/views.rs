//! Saved views: named `/` filters of a queue (`saved_views`, migration 0010).

use fl_types::{QueueKind, SavedView};
use rusqlite::{Connection, params};

/// Every saved view, by kind (display order of `QueueKind::ALL`), then name.
pub fn list(conn: &Connection) -> rusqlite::Result<Vec<SavedView>> {
    let rows = conn
        .prepare_cached("SELECT id, name, kind, query FROM saved_views ORDER BY name COLLATE NOCASE, id")?
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut views: Vec<SavedView> = rows
        .into_iter()
        .filter_map(|(id, name, kind, query)| Some(SavedView { id, name, kind: QueueKind::parse(&kind)?, query }))
        .collect();
    views.sort_by_key(|v| QueueKind::ALL.iter().position(|k| *k == v.kind));
    Ok(views)
}

/// Save `query` as `name` for `kind`; an existing view of that name is
/// replaced. Returns the view's id.
pub fn save(conn: &Connection, kind: QueueKind, name: &str, query: &str, now: i64) -> rusqlite::Result<i64> {
    conn.prepare_cached(
        "INSERT INTO saved_views(name, kind, query, created_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(kind, name) DO UPDATE SET query = excluded.query
         RETURNING id",
    )?
    .query_row(params![name, kind.as_str(), query, now], |r| r.get(0))
}

pub fn delete(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.prepare_cached("DELETE FROM saved_views WHERE id = ?1")?.execute([id])?;
    Ok(())
}
