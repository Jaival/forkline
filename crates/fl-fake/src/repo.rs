//! Fixture data owned by agent 2 (languages, contributors, releases, commits, star history, starred).
//! Runs after the base repos / issues / pr_details rows exist.
//! Keep it deterministic: derive every value from ids, no randomness.

use fl_db::rusqlite::{Result, Transaction};

pub fn seed(_tx: &Transaction<'_>) -> Result<()> {
    Ok(())
}
