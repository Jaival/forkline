//! Local git: gitoxide diffs, merge-base, blobless clone/fetch manager. Owner: Phase 1 agent 5.
//!
//! Contract (consumed by agent 4's `diff_*` commands): the signatures below
//! are fixed; agent 5 replaces the bodies. Clones live under
//! `<data_dir>/git/<owner>/<name>.git` (bare, blobless).

use std::path::{Path, PathBuf};

use fl_types::{DiffFile, DiffSummary};

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// The commits are not local yet and could not be fetched.
    #[error("commits not available: {0}")]
    Missing(String),
    #[error("git: {0}")]
    Other(String),
}

pub type GitResult<T> = Result<T, GitError>;

/// Owns the internal clones. Cheap to share behind an `Arc`.
pub struct GitStore {
    root: PathBuf,
}

impl GitStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("git"),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Make sure `oids` exist in the local clone of `owner/name`, cloning or
    /// fetching as needed. `token` authenticates HTTPS for private repos.
    /// Returns the clone path. Blocking; call it off the async runtime.
    pub fn ensure_commits(
        &self,
        owner: &str,
        name: &str,
        token: Option<&str>,
        oids: &[&str],
    ) -> GitResult<PathBuf> {
        let _ = (owner, name, token, oids);
        Err(GitError::Other("ensure_commits not implemented".into()))
    }

    /// Changed files between `merge-base(base, head)` and `head`.
    pub fn diff_summary(&self, clone: &Path, base: &str, head: &str) -> GitResult<DiffSummary> {
        let _ = (clone, base, head);
        Err(GitError::Other("diff_summary not implemented".into()))
    }

    /// Hunks for one file in the same range as `diff_summary`.
    pub fn diff_file(&self, clone: &Path, base: &str, head: &str, path: &str) -> GitResult<DiffFile> {
        let _ = (clone, base, head, path);
        Err(GitError::Other("diff_file not implemented".into()))
    }
}
