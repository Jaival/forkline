//! Diff DTOs. Produced by `fl-git` (agent 5), served by `diff_*` commands
//! and rendered by the diff viewer (agent 4).
//!
//! A PR diff is `merge-base(base_oid, head_oid)..head_oid`, like GitHub's
//! "Files changed" tab.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
}

/// One changed file, without its hunks (file tree / list).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffFileSummary {
    pub path: String,
    /// Previous path for renames.
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
}

/// List of changed files for a PR.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    pub merge_base_oid: String,
    pub head_oid: String,
    pub files: Vec<DiffFileSummary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: LineKind,
    /// Line number in the old file (`None` for added lines).
    pub old_no: Option<u32>,
    /// Line number in the new file (`None` for deleted lines).
    pub new_no: Option<u32>,
    /// Line text without the trailing newline or the +/- marker.
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    /// Text after the second `@@` (function context), may be empty.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// Full diff of one file. Binary files have no hunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffFile {
    pub summary: DiffFileSummary,
    pub hunks: Vec<DiffHunk>,
}
