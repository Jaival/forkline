//! DTOs owned by agent 4 (PR detail, reviews, threads, checks).
//! Other agents: do not edit; ask the owner.

use serde::{Deserialize, Serialize};

use crate::{DiffSide, IssueRow, Label, Timestamp};

/// Everything the PR view needs except review threads (see `ReviewThread`).
/// Comments are capped at `PAGE_MAX`; `row.comments` is the full count so
/// the UI can say "n more on GitHub".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrDetail {
    pub row: IssueRow,
    pub body_html: Option<String>,
    pub body_md: Option<String>,
    pub labels: Vec<Label>,
    pub created_at: Timestamp,
    pub base_ref: String,
    pub head_ref: String,
    pub base_oid: String,
    pub head_oid: String,
    pub is_draft: bool,
    /// MERGEABLE | CONFLICTING | UNKNOWN
    pub mergeable: Option<String>,
    /// GitHub mergeStateStatus.
    pub merge_state: Option<String>,
    /// APPROVED | CHANGES_REQUESTED | REVIEW_REQUIRED
    pub review_decision: Option<String>,
    /// SUCCESS | FAILURE | PENDING | ERROR
    pub check_state: Option<String>,
    pub additions: u32,
    pub deletions: u32,
    pub changed_files: u32,
    pub review_requests: Vec<String>,
    /// Fields of this PR with an unconfirmed optimistic change (e.g. `state`).
    pub pending_fields: Vec<String>,
    pub comments: Vec<PrComment>,
    pub reviews: Vec<PrReview>,
    pub checks: Vec<PrCheck>,
    pub commits: Vec<PrCommit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrComment {
    /// Negative = local optimistic comment.
    pub id: i64,
    pub author: String,
    pub body_html: Option<String>,
    pub body_md: Option<String>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrReview {
    pub id: i64,
    pub author: String,
    /// APPROVED | CHANGES_REQUESTED | COMMENTED | PENDING | DISMISSED
    pub state: String,
    pub body_html: Option<String>,
    pub body_md: Option<String>,
    pub submitted_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrCheck {
    pub id: i64,
    pub name: String,
    /// QUEUED | IN_PROGRESS | COMPLETED
    pub status: String,
    /// SUCCESS | FAILURE | NEUTRAL | CANCELLED | SKIPPED | TIMED_OUT
    pub conclusion: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrCommit {
    pub oid: String,
    pub headline: String,
    pub author: Option<String>,
    pub authored_at: Option<Timestamp>,
}

/// One inline review thread. `line` is `None` when the thread is outdated
/// and no longer maps to a line of the current diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewThread {
    pub id: i64,
    pub path: String,
    pub side: DiffSide,
    pub line: Option<u32>,
    pub start_line: Option<u32>,
    pub original_line: Option<u32>,
    pub is_resolved: bool,
    pub is_outdated: bool,
    /// An unconfirmed resolve / unresolve is queued.
    pub pending: bool,
    pub comments: Vec<ReviewThreadComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewThreadComment {
    pub id: i64,
    pub author: String,
    pub body_html: Option<String>,
    pub body_md: Option<String>,
    pub created_at: Timestamp,
}
