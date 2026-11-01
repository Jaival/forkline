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
        })
    })
}
