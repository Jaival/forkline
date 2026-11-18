//! DTOs owned by agent 1 (auth state, sync progress, action queue).
//! Other agents: do not edit; ask the owner.

use serde::{Deserialize, Serialize};

use crate::{ActionId, Timestamp};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthState {
    pub signed_in: bool,
    /// Viewer login, once known.
    pub login: Option<String>,
    /// False when the build has no `FORKLINE_GITHUB_CLIENT_ID`; device-flow
    /// sign-in is then unavailable and only `FORKLINE_GITHUB_TOKEN` works.
    pub client_id_configured: bool,
    /// True when `FORKLINE_GITHUB_TOKEN` supplies the token.
    pub env_override: bool,
}

/// Device-flow code to show the user. The device code itself stays in Rust.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u32,
    /// Seconds to wait between `auth_poll` calls.
    pub interval: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AuthPoll {
    /// Not approved yet; poll again after `interval` seconds.
    Pending { interval: u32 },
    Done { login: String },
    Expired,
    Denied,
}

/// A write action that failed for good. Its optimistic change was rolled back.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedAction {
    pub id: ActionId,
    /// Short human text, e.g. "Comment on owner/repo#12".
    pub summary: String,
    pub error: String,
    pub created_at: Timestamp,
}
