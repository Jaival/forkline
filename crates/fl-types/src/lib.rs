//! Shared data types between the backend crates and the UI.
//!
//! With the `specta` feature (Tauri app only) every type derives
//! `specta::Type`, and `tauri-specta` exports them to `ui/src/bindings.ts`.
//! IDs and timestamps are `i64`.
//!
//! Timestamps are unix seconds (UTC).

use serde::{Deserialize, Serialize};

// One module per Phase 1 area (see docs/PHASE1.md). Each agent adds its
// DTOs to its own module only; shared contracts live in `action` and `diff`.
pub mod action;
pub mod diff;
pub mod lists;
pub mod pr;
pub mod queue;
pub mod repo;
pub mod search;
pub mod sync;

pub use action::*;
pub use diff::*;
pub use lists::*;
pub use pr::*;
pub use queue::*;
pub use repo::*;
pub use search::*;
pub use sync::*;

pub type RepoId = i64;
pub type IssueId = i64;
pub type UserId = i64;
pub type Timestamp = i64;

/// Maximum rows returned by one paged command.
pub const PAGE_MAX: u32 = 100;

/// One page of rows. `next` is the offset of the following page, if any.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PageReq {
    pub offset: u32,
    pub limit: u32,
}

impl PageReq {
    /// Clamp the limit to `PAGE_MAX` (boundary validation for UI input).
    pub fn clamped(self) -> Self {
        Self {
            offset: self.offset,
            limit: self.limit.clamp(1, PAGE_MAX),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    Open,
    Closed,
    Merged,
}

impl IssueState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "closed" => Some(Self::Closed),
            "merged" => Some(Self::Merged),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RepoSummary {
    pub id: RepoId,
    pub owner: String,
    pub name: String,
    pub pinned: bool,
    pub open_prs: u32,
    pub open_issues: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct Label {
    pub id: i64,
    pub name: String,
    pub color: String,
}

/// One row in any issue / PR list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct IssueRow {
    pub id: IssueId,
    pub repo_id: RepoId,
    pub repo: String,
    pub number: i64,
    pub is_pr: bool,
    pub state: IssueState,
    pub title: String,
    pub author: String,
    pub comments: u32,
    pub updated_at: Timestamp,
    pub unread: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct IssueDetail {
    pub row: IssueRow,
    pub body_html: Option<String>,
    pub body_md: Option<String>,
    pub labels: Vec<Label>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SyncStatus {
    pub last_synced_at: Option<Timestamp>,
    pub pending_actions: u32,
    pub failed_actions: u32,
    pub paused: bool,
    pub error: Option<String>,
}

/// Emitted when sync or an action changed rows. The UI invalidates queries
/// whose key starts with one of `scopes` (e.g. "issues", "repos").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DataChanged {
    pub scopes: Vec<String>,
}

/// Error type returned by every command.
#[derive(Debug, thiserror::Error, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", content = "message")]
pub enum CmdError {
    #[error("database error: {0}")]
    Db(String),
    #[error("not found")]
    NotFound,
    #[error("not implemented yet: {0}")]
    NotImplemented(String),
    /// No GitHub token: the UI shows the sign-in flow.
    #[error("not signed in")]
    Unauthorized,
    /// GitHub or the network failed; `message` is safe to show.
    #[error("github: {0}")]
    Github(String),
    #[error("{0}")]
    Other(String),
}

pub type CmdResult<T> = Result<T, CmdError>;
