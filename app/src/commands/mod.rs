//! Tauri commands, one file per feature. Each file is owned by one Phase 1
//! agent (see docs/HANDOFF.md). Register new commands in `lib.rs`.
//!
//! All DB commands are `async` so they run on the async runtime, not the
//! main (UI event loop) thread.

pub mod actions;
pub mod bench;
pub mod diff;
pub mod inbox;
pub mod issue;
pub mod layout;
pub mod lists;
pub mod pr;
pub mod repo;
pub mod search;

use fl_types::CmdError;

pub fn db_err(e: fl_db::rusqlite::Error) -> CmdError {
    CmdError::Db(e.to_string())
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

pub fn not_implemented<T>(what: &str) -> Result<T, CmdError> {
    Err(CmdError::NotImplemented(what.into()))
}
