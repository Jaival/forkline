//! Saved UI layout (tabs + panes) and settings. The layout JSON is owned and
//! validated by the UI; Rust stores it as text.

use rusqlite::{Connection, OptionalExtension};

pub fn get(conn: &Connection) -> rusqlite::Result<Option<String>> {
    conn.prepare_cached("SELECT json FROM ui_layout WHERE id = 1")?
        .query_row([], |r| r.get(0))
        .optional()
}

pub fn save(conn: &Connection, json: &str) -> rusqlite::Result<()> {
    conn.prepare_cached(
        "INSERT INTO ui_layout (id, json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET json = excluded.json",
    )?
    .execute([json])?;
    Ok(())
}

pub fn setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.prepare_cached("SELECT value_json FROM settings WHERE key = ?1")?
        .query_row([key], |r| r.get(0))
        .optional()
}

pub fn set_setting(conn: &Connection, key: &str, value_json: &str) -> rusqlite::Result<()> {
    conn.prepare_cached(
        "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
    )?
    .execute([key, value_json])?;
    Ok(())
}
