//! DTOs owned by agent 3 (list filters, inbox rows, issue timeline, comments).
//! Other agents: do not edit; ask the owner.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{IssueId, IssueRow, IssueState, Label, RepoId, Timestamp, UserId};

/// Filter for `issue_list`. Every field narrows the result; `None` / `false` means "any".
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct IssueFilter {
    pub repo_id: Option<RepoId>,
    pub state: Option<IssueState>,
    /// Author login.
    pub author: Option<String>,
    /// Assignee login.
    pub assignee: Option<String>,
    /// Label name.
    pub label: Option<String>,
    pub is_pr: Option<bool>,
    /// Open PRs with a review request for the signed-in user (`viewer` setting).
    pub review_requested: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct InboxFilter {
    pub repo_id: Option<RepoId>,
    pub unread_only: bool,
    /// Order by repo first so the UI can render repo groups.
    pub grouped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct InboxRow {
    /// GitHub notification thread id.
    pub id: String,
    pub repo_id: RepoId,
    pub repo: String,
    pub subject_type: String,
    /// Linked cached issue / PR, if any.
    pub issue_id: Option<IssueId>,
    pub is_pr: bool,
    pub title: String,
    pub reason: String,
    pub unread: bool,
    pub updated_at: Timestamp,
    /// A read / done action for this thread is queued and not yet confirmed.
    pub pending: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct User {
    pub id: UserId,
    pub login: String,
}

/// A comment. Local optimistic comments have a negative `id`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CommentRow {
    pub id: i64,
    pub author: String,
    pub body_html: Option<String>,
    pub body_md: Option<String>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TimelineEvent {
    pub id: i64,
    pub kind: String,
    pub actor: String,
    pub created_at: Timestamp,
    pub payload_json: Option<String>,
}

/// Everything under the issue header: assignees, conversation, timeline.
/// Comments and events are capped at 100 each (oldest first).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct IssueThread {
    pub assignees: Vec<User>,
    pub comments: Vec<CommentRow>,
    pub timeline: Vec<TimelineEvent>,
    /// PRs linked to an issue, or issues linked to a PR.
    pub linked: Vec<IssueRow>,
    /// Fields of the issue row with an unsent action: `state`, `labels`, `assignees`.
    pub pending_fields: Vec<String>,
}

/// Choices for the label and assignee editors.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EditOptions {
    pub labels: Vec<Label>,
    pub users: Vec<User>,
}
