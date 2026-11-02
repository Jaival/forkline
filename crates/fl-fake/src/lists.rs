//! Fixture data owned by agent 3 (labels, assignees, comments, timeline, notifications).
//! Runs after the base repos / issues / pr_details rows exist.
//! Keep it deterministic: derive every value from ids, no randomness.

use fl_db::rusqlite::{Result, Transaction};

pub fn seed(_tx: &Transaction<'_>) -> Result<()> {
    Ok(())
}
