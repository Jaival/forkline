//! Migrations keyed by `PRAGMA user_version`.

use rusqlite::Connection;

/// Index = version - 1. Append only; never edit a shipped migration.
const MIGRATIONS: &[&str] = &[include_str!("../migrations/0001_init.sql")];

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
