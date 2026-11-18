//! Rows of the work queue: the grouped list behind Inbox, Pull requests,
//! Issues and Review requests (GPUI G1, `docs/design/ui-plan.md`).

use serde::{Deserialize, Serialize};

use crate::{IssueId, IssueState, RepoId, Timestamp};

/// Which list a queue shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QueueKind {
    /// Notifications that are not done, grouped by reason.
    Inbox,
    /// Open pull requests.
    Pulls,
    /// Open issues.
    Issues,
    /// Open pull requests with a review request for the signed-in user.
    Reviews,
}

impl QueueKind {
    pub const ALL: [Self; 4] = [Self::Inbox, Self::Pulls, Self::Issues, Self::Reviews];

    /// Stable text form, stored in `saved_views.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Pulls => "pulls",
            Self::Issues => "issues",
            Self::Reviews => "reviews",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// A named `/` filter of one queue kind (`saved_views`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedView {
    pub id: i64,
    pub name: String,
    pub kind: QueueKind,
    /// `fl-search` query text, e.g. `is:open label:bug crash`.
    pub query: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QueueQuery {
    pub kind: QueueKind,
    /// Only this repository; `None` means all.
    pub repo_id: Option<RepoId>,
    /// One list, newest first, no group headers. Only the Inbox has more
    /// than one group, so only its order changes.
    #[serde(default)]
    pub flat: bool,
}

/// Section of a queue. Declaration order is display order; a query returns
/// rows ordered by group, then newest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum QueueGroup {
    /// Inbox reason `review_requested`; also the only group of `Reviews`.
    ReviewRequested,
    /// Inbox reasons `mention` and `team_mention`.
    Mentioned,
    /// Inbox reason `assign`.
    Assigned,
    /// Inbox reason `author`.
    YourPrs,
    /// Every other inbox reason (subscribed, comment, state_change, ...).
    Other,
    /// The only group of `Pulls` and `Issues`.
    Open,
}

impl QueueGroup {
    pub const ALL: [Self; 6] =
        [Self::ReviewRequested, Self::Mentioned, Self::Assigned, Self::YourPrs, Self::Other, Self::Open];

    pub fn label(self) -> &'static str {
        match self {
            Self::ReviewRequested => "Review requested",
            Self::Mentioned => "Mentioned",
            Self::Assigned => "Assigned",
            Self::YourPrs => "Your PRs",
            Self::Other => "Other",
            Self::Open => "Open",
        }
    }
}

/// Combined check result of a PR's head commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckState {
    Success,
    /// `FAILURE` and `ERROR`.
    Failure,
    Pending,
}

impl CheckState {
    /// Parse `pr_details.check_state`.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "SUCCESS" => Some(Self::Success),
            "FAILURE" | "ERROR" => Some(Self::Failure),
            "PENDING" => Some(Self::Pending),
            _ => None,
        }
    }
}

/// One row of a queue. An inbox row without a cached issue or PR (a release,
/// a discussion) has `issue_id`, `number`, `state` and `author` unset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueRow {
    /// Unique within one queue: the notification thread id in Inbox, the
    /// issue id (as text) in the other lists.
    pub key: String,
    pub group: QueueGroup,
    pub issue_id: Option<IssueId>,
    pub repo_id: RepoId,
    /// `owner/name`.
    pub repo: String,
    pub number: Option<i64>,
    pub is_pr: bool,
    pub state: Option<IssueState>,
    pub title: String,
    pub author: Option<String>,
    /// PRs only.
    pub checks: Option<CheckState>,
    pub updated_at: Timestamp,
    pub unread: bool,
}
