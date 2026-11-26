//! Sync status for the footer. Owner: agent 1.

use fl_types::SyncStatus;
use rusqlite::Connection;

pub fn status(conn: &Connection) -> rusqlite::Result<SyncStatus> {
    conn.prepare_cached(
        "SELECT (SELECT max(last_synced_at) FROM sync_state),
                (SELECT count(*) FROM action_queue WHERE state = 'pending'),
                (SELECT count(*) FROM action_queue WHERE state = 'failed')",
    )?
    .query_row([], |r| {
        Ok(SyncStatus {
            last_synced_at: r.get(0)?,
            pending_actions: r.get(1)?,
            failed_actions: r.get(2)?,
            paused: false,
            error: None,
            repos_synced: 0,
            repos_pinned: 0,
            note: None,
        })
    })
    .and_then(|mut s| {
        (s.repos_synced, s.repos_pinned) = deep_progress(conn)?;
        Ok(s)
    })
}

/// `(done, pinned)`: pinned repos that finished a deep sync, and all pinned
/// repos. A deep sync stamps `repo:<id>` when it ends.
pub fn deep_progress(conn: &Connection) -> rusqlite::Result<(u32, u32)> {
    conn.prepare_cached(
        "SELECT (SELECT count(*) FROM repos r WHERE r.pinned = 1 AND EXISTS
                   (SELECT 1 FROM sync_state s WHERE s.key = 'repo:' || r.id AND s.last_synced_at IS NOT NULL)),
                (SELECT count(*) FROM repos WHERE pinned = 1)",
    )?
    .query_row([], |r| Ok((r.get(0)?, r.get(1)?)))
}
