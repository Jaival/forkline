//! GitHub auth, GraphQL/REST client, sync engine, action queue, rate limits. Owner: Phase 1 agent 1.
//!
//! Contract for other agents: `auth::token()` is the only API they call
//! (agent 4 / 5 need it to fetch private repos). Signature is fixed.
//!
//! - `auth`: token lookup, credential store, device flow.
//! - `github`: HTTP client, error classification, rate-limit headers.
//! - `queue`: optimistic write actions (`pending_fields`), send, rollback.
//! - `sync`: repo metadata, deep sync of pinned repos, notifications.
//! - `engine`: background loops tying it together; the app holds one.

pub mod auth;
pub mod engine;
pub mod github;
pub mod queue;
pub mod sync;

pub use engine::{Config, Engine};

use fl_types::CmdError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
    /// The action or input is not valid; never retried.
    #[error("{0}")]
    Invalid(String),
    /// No token, or GitHub rejected it (401).
    #[error("not signed in")]
    Unauthorized,
    #[error("network: {0}")]
    Network(String),
    /// Primary or secondary rate limit. `until` is unix seconds.
    #[error("GitHub rate limit")]
    RateLimited { until: i64 },
    #[error("GitHub {status}: {message}")]
    Api { status: u16, message: String },
}

impl Error {
    /// Network errors, 5xx and rate limits are worth another try.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network(_) | Self::RateLimited { .. } => true,
            Self::Api { status, .. } => *status >= 500,
            _ => false,
        }
    }
}

impl From<Error> for CmdError {
    fn from(e: Error) -> Self {
        match e {
            Error::Db(e) => CmdError::Db(e.to_string()),
            Error::Invalid(m) => CmdError::Other(m),
            Error::Unauthorized => CmdError::Unauthorized,
            e => CmdError::Github(e.to_string()),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Run `f` on the writer connection, off the async runtime.
pub(crate) async fn write<T: Send + 'static>(
    db: &std::sync::Arc<fl_db::Db>,
    f: impl FnOnce(&mut rusqlite::Connection) -> Result<T> + Send + 'static,
) -> Result<T> {
    let db = db.clone();
    tokio::task::spawn_blocking(move || db.write(|c| Ok(f(c))))
        .await
        .map_err(|e| Error::Invalid(format!("database task failed: {e}")))??
}

/// Run `f` on a reader connection, off the async runtime.
pub(crate) async fn read<T: Send + 'static>(
    db: &std::sync::Arc<fl_db::Db>,
    f: impl FnOnce(&rusqlite::Connection) -> Result<T> + Send + 'static,
) -> Result<T> {
    let db = db.clone();
    tokio::task::spawn_blocking(move || db.read(|c| Ok(f(c))))
        .await
        .map_err(|e| Error::Invalid(format!("database task failed: {e}")))??
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Parse a GitHub ISO 8601 UTC timestamp (`2024-01-02T03:04:05Z`) to unix
/// seconds. Returns 0 for anything else (GitHub always sends this form).
pub fn parse_ts(s: &str) -> i64 {
    let n = |r: std::ops::Range<usize>| s.get(r).and_then(|v| v.parse::<i64>().ok());
    let (Some(y), Some(m), Some(d), Some(hh), Some(mm), Some(ss)) =
        (n(0..4), n(5..7), n(8..10), n(11..13), n(14..16), n(17..19))
    else {
        return 0;
    };
    // Days from civil (Howard Hinnant).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + hh * 3600 + mm * 60 + ss
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_github_timestamps() {
        assert_eq!(super::parse_ts("1970-01-01T00:00:00Z"), 0);
        assert_eq!(super::parse_ts("2024-02-29T12:30:15Z"), 1_709_209_815);
        assert_eq!(super::parse_ts("garbage"), 0);
    }
}
