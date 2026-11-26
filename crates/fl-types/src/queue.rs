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
    /// Home: open items grouped by what the signed-in user has to do.
    Home,
}

impl QueueKind {
    pub const ALL: [Self; 5] = [Self::Inbox, Self::Pulls, Self::Issues, Self::Reviews, Self::Home];

    /// Stable text form, stored in `saved_views.kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Pulls => "pulls",
            Self::Issues => "issues",
            Self::Reviews => "reviews",
            Self::Home => "home",
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
    /// Home: my PRs with failed checks, requested changes or conflicts.
    NeedsAction,
    /// Home: review requests, and other people's PRs on my repos.
    WaitingOnMe,
    /// Home: assigned to me, or an unread mention / assign notification.
    AssignedMentions,
    /// Home: my PRs with green checks, no blocking review, no conflicts.
    ReadyToMerge,
    /// Home: my other open PRs (checks pending, waiting, drafts).
    InProgress,
    /// Home: every other open item, newest first.
    Recent,
}

impl QueueGroup {
    pub const ALL: [Self; 12] = [
        Self::ReviewRequested,
        Self::Mentioned,
        Self::Assigned,
        Self::YourPrs,
        Self::Other,
        Self::Open,
        Self::NeedsAction,
        Self::WaitingOnMe,
        Self::AssignedMentions,
        Self::ReadyToMerge,
        Self::InProgress,
        Self::Recent,
    ];

    /// Groups shown folded until the user opens them.
    pub fn starts_collapsed(self) -> bool {
        matches!(self, Self::InProgress | Self::Recent)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ReviewRequested => "Review requested",
            Self::Mentioned => "Mentioned",
            Self::Assigned => "Assigned",
            Self::YourPrs => "Your PRs",
            Self::Other => "Other",
            Self::Open => "Open",
            Self::NeedsAction => "Needs action",
            Self::WaitingOnMe => "Waiting on me",
            Self::AssignedMentions => "Assigned and mentions",
            Self::ReadyToMerge => "Ready to merge",
            Self::InProgress => "My PRs in progress",
            Self::Recent => "Recently updated",
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

/// What changed on an unread item, from cached data: one short reason
/// shown in place of the bare unread dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowReason {
    ChecksFailed,
    ChangesRequested,
    Approved,
    /// Comments created since the item was last opened.
    NewComments(u32),
    /// Commits authored since the PR was last opened.
    NewCommits,
}

impl RowReason {
    /// The first reason that applies, most urgent first. Only an unread item
    /// that was opened before has one.
    pub fn pick(
        unread: bool,
        checks: Option<CheckState>,
        review_decision: Option<&str>,
        new_comments: u32,
        new_commits: bool,
    ) -> Option<Self> {
        if !unread {
            return None;
        }
        if checks == Some(CheckState::Failure) {
            return Some(Self::ChecksFailed);
        }
        match review_decision {
            Some("CHANGES_REQUESTED") => return Some(Self::ChangesRequested),
            Some("APPROVED") => return Some(Self::Approved),
            _ => {}
        }
        if new_comments > 0 {
            return Some(Self::NewComments(new_comments));
        }
        new_commits.then_some(Self::NewCommits)
    }

    pub fn label(self) -> String {
        match self {
            Self::ChecksFailed => "checks failed".into(),
            Self::ChangesRequested => "changes requested".into(),
            Self::Approved => "approved".into(),
            Self::NewComments(1) => "1 new comment".into(),
            Self::NewComments(n) => format!("{n} new comments"),
            Self::NewCommits => "new commits".into(),
        }
    }
}

#[cfg(test)]
mod reason_tests {
    use super::*;

    #[test]
    fn the_most_urgent_reason_wins_and_read_rows_have_none() {
        let pick = RowReason::pick;
        assert_eq!(pick(false, Some(CheckState::Failure), None, 3, true), None);
        assert_eq!(pick(true, Some(CheckState::Failure), Some("APPROVED"), 3, true), Some(RowReason::ChecksFailed));
        assert_eq!(pick(true, None, Some("CHANGES_REQUESTED"), 3, true), Some(RowReason::ChangesRequested));
        assert_eq!(pick(true, Some(CheckState::Success), Some("APPROVED"), 0, false), Some(RowReason::Approved));
        assert_eq!(pick(true, None, Some("REVIEW_REQUIRED"), 2, true), Some(RowReason::NewComments(2)));
        assert_eq!(pick(true, None, None, 0, true), Some(RowReason::NewCommits));
        assert_eq!(pick(true, None, None, 0, false), None);
        assert_eq!(RowReason::NewComments(1).label(), "1 new comment");
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
    /// What changed since the item was last opened (unread rows only).
    #[serde(default)]
    pub reason: Option<RowReason>,
    /// The author is a GitHub App (dependabot, renovate, ...).
    #[serde(default)]
    pub bot: bool,
}
