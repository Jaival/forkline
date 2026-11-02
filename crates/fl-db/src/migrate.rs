//! Migrations keyed by `PRAGMA user_version`.

use rusqlite::Connection;

/// Index = version - 1. Append only; never edit a shipped migration.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_search_docs_inbox.sql"),
    include_str!("../migrations/0003_phase1_shared.sql"),
    include_str!("../migrations/0004_sync.sql"),
    // Phase 1: each agent appends one line in its own number slot:
    // 0004 sync (agent 1), 0005 repo (agent 2), 0006 lists/inbox/issue
    // (agent 3), 0007 pr (agent 4), 0008 search/git (agent 5).
    // The integrator merges them in number order.
];

pub const LATEST: i64 = MIGRATIONS.len() as i64;

pub fn run(conn: &mut Connection) -> rusqlite::Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i as i64 + 1)?;
        tx.commit()?;
    }
    Ok(())
}
