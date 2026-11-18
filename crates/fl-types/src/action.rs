//! Write actions. The UI calls `action_enqueue(Action)`; agent 1's action
//! queue applies the change optimistically to SQLite, records the old value
//! in `pending_fields`, sends it to GitHub, and retries or rolls back.
//!
//! Contract shared by agents 1 (queue), 3 (issue/inbox UI) and 4 (PR UI).
//! Add variants only in coordination with agent 1.

use serde::{Deserialize, Serialize};

use crate::{IssueId, UserId};

pub type ActionId = i64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// New comment. Shown at once as a local comment with a negative id.
    Comment { issue_id: IssueId, body_md: String },
    /// Close or reopen an issue / PR. `state` must be `open` or `closed`.
    SetState { issue_id: IssueId, state: crate::IssueState },
    /// Replace the full label set.
    SetLabels { issue_id: IssueId, label_ids: Vec<i64> },
    /// Replace the full assignee set.
    SetAssignees { issue_id: IssueId, user_ids: Vec<UserId> },
    /// Submit a review with optional inline comments.
    SubmitReview {
        pr_id: IssueId,
        event: ReviewEvent,
        body_md: String,
        comments: Vec<DraftReviewComment>,
    },
    /// Reply in an existing review thread.
    ReplyThread { thread_id: i64, body_md: String },
    /// Resolve or unresolve a review thread.
    ResolveThread { thread_id: i64, resolved: bool },
    Merge { pr_id: IssueId, method: MergeMethod },
    /// Notification thread: mark read.
    NotificationRead { id: String },
    /// Notification thread: mark done (removed from the inbox).
    NotificationDone { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewEvent {
    Approve,
    RequestChanges,
    Comment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    /// Stable text form, as serde writes it; stored in `repo_merge_methods`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Merge, Self::Squash, Self::Rebase].into_iter().find(|m| m.as_str() == s)
    }
}

/// Inline comment in a review. Lines are file line numbers on `side`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftReviewComment {
    pub path: String,
    pub side: DiffSide,
    pub line: u32,
    /// Set for a multi-line comment; must be < `line`.
    pub start_line: Option<u32>,
    pub body_md: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DiffSide {
    Left,
    Right,
}
