//! DTOs owned by agent 2 (repo overview, repo lists).
//! Other agents: do not edit; ask the owner.

use serde::{Deserialize, Serialize};

use crate::{RepoId, Timestamp};

/// One row in the Repositories and Starred views.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RepoRow {
    pub id: RepoId,
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stars: u32,
    pub pinned: bool,
    pub starred: bool,
    pub open_prs: u32,
    pub open_issues: u32,
}

/// Filter for `repos_list`. `query` matches `owner/name` (case-insensitive substring).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RepoFilter {
    pub query: String,
    pub starred_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LanguageShare {
    pub name: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct CommitRow {
    pub oid: String,
    pub headline: String,
    pub author: Option<String>,
    pub authored_at: Timestamp,
    /// SUCCESS | FAILURE | PENDING | ERROR, if known.
    pub check_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ReleaseRow {
    pub tag: String,
    pub name: Option<String>,
    pub published_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ContributorRow {
    pub login: String,
    pub contributions: u32,
}

/// Star count on one day (`day` = unix seconds / 86400).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct StarPoint {
    pub day: i64,
    pub stars: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct RepoOverview {
    pub id: RepoId,
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub default_branch: String,
    pub license: Option<String>,
    pub stars: u32,
    pub forks: u32,
    pub pinned: bool,
    pub starred: bool,
    pub open_prs: u32,
    pub open_issues: u32,
    pub languages: Vec<LanguageShare>,
    pub commits: Vec<CommitRow>,
    pub releases: Vec<ReleaseRow>,
    pub contributors: Vec<ContributorRow>,
    pub star_history: Vec<StarPoint>,
}

/// One repo in the Explore view: starred lately by people the user follows.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ExploreRow {
    pub repo_id: RepoId,
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub language: Option<String>,
    pub stars: u32,
    /// Starred by the user too.
    pub starred: bool,
    pub pinned: bool,
    /// Followed users who starred it, newest first, at most three.
    pub starred_by: Vec<String>,
    pub starred_by_count: u32,
    pub last_starred_at: Timestamp,
}
