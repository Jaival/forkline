//! Local git for PR diffs, built on the `git` CLI (Git for Windows is a stated
//! requirement). Owner: Phase 1 agent 5.
//!
//! Clones live under `<data_dir>/git/<owner>/<name>.git` (bare, blobless).
//! Network commands get the token through environment variables only
//! (`GIT_CONFIG_*` with `http.extraHeader`): it never appears in argv, URLs,
//! git config files, logs or errors. Offline commands set `GIT_NO_LAZY_FETCH`
//! so a missing blob fails fast instead of hitting the network;
//! `ensure_commits` therefore prefetches the blobs a PR diff needs.
//!
//! Contract (consumed by agent 4's `diff_*` commands): the signatures of
//! `GitStore` are fixed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use base64::Engine;
use fl_types::{DiffFile, DiffFileSummary, DiffSummary};

mod parse;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// The commits are not local yet and could not be fetched.
    #[error("commits not available: {0}")]
    Missing(String),
    /// GitHub refused the credentials (or asked for some and got none).
    #[error("GitHub rejected the git credentials")]
    Unauthorized,
    #[error("git: {0}")]
    Other(String),
}

pub type GitResult<T> = Result<T, GitError>;

/// Owns the internal clones. Cheap to share behind an `Arc`.
pub struct GitStore {
    root: PathBuf,
    remote_base: String,
    locks: Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
}

/// Run `git` with `args` (in `dir` if given). `token` is sent as an HTTP
/// header through env; `offline` forbids lazy fetches. Returns stdout.
fn run_git(dir: Option<&Path>, args: &[&str], token: Option<&str>, offline: bool) -> GitResult<Vec<u8>> {
    let mut cmd = Command::new("git");
    if let Some(d) = dir {
        cmd.arg("-C").arg(d);
    }
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("GIT_TERMINAL_PROMPT", "0")
        // Git for Windows enables Git Credential Manager system-wide; on a 401
        // it would open a sign-in window. An empty helper clears the list.
        .env("GCM_INTERACTIVE", "never")
        .env("GIT_CONFIG_KEY_0", "credential.helper")
        .env("GIT_CONFIG_VALUE_0", "");
    if offline {
        cmd.env("GIT_NO_LAZY_FETCH", "1");
    }
    if let Some(t) = token {
        let basic = base64::engine::general_purpose::STANDARD.encode(format!("x-access-token:{t}"));
        cmd.env("GIT_CONFIG_COUNT", "2")
            .env("GIT_CONFIG_KEY_1", "http.extraHeader")
            .env("GIT_CONFIG_VALUE_1", format!("Authorization: Basic {basic}"));
    } else {
        cmd.env("GIT_CONFIG_COUNT", "1");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd
        .output()
        .map_err(|e| GitError::Other(format!("cannot run git: {e}")))?;
    if out.status.success() {
        return Ok(out.stdout);
    }
    let mut msg = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    if let Some(t) = token {
        msg = msg.replace(t, "***");
    }
    // With prompts off, a 401 shows up as one of these lines.
    if ["Authentication failed", "could not read Username", "Invalid username or token"].iter().any(|m| msg.contains(m)) {
        return Err(GitError::Unauthorized);
    }
    Err(GitError::Other(msg))
}

fn is_oid(s: &str) -> bool {
    (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_name(s: &str) -> bool {
    s != "."
        && s != ".."
        && !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

impl GitStore {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("git"),
            remote_base: "https://github.com".into(),
            locks: Mutex::default(),
        }
    }

    /// Clone from `<base>/<owner>/<name>.git` instead of GitHub (tests).
    pub fn with_remote_base(mut self, base: impl Into<String>) -> Self {
        self.remote_base = base.into();
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Make sure `oids` exist in the local clone of `owner/name`, cloning or
    /// fetching as needed. `token` authenticates HTTPS for private repos.
    /// Also prefetches the blobs needed to diff the oids against their
    /// merge-base, so `diff_*` work offline. Returns the clone path.
    /// Blocking; call it off the async runtime.
    pub fn ensure_commits(
        &self,
        owner: &str,
        name: &str,
        token: Option<&str>,
        oids: &[&str],
    ) -> GitResult<PathBuf> {
        if !is_name(owner) || !is_name(name) {
            return Err(GitError::Other(format!("invalid repository {owner}/{name}")));
        }
        if let Some(bad) = oids.iter().find(|o| !is_oid(o)) {
            return Err(GitError::Other(format!("invalid object id {bad}")));
        }
        let path = self.root.join(owner).join(format!("{name}.git"));
        let lock = Arc::clone(
            self.locks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry(path.clone())
                .or_default(),
        );
        let _guard = lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

        let has = |oid: &str| run_git(Some(&path), &["cat-file", "-e", &format!("{oid}^{{commit}}")], None, true).is_ok();
        if !path.join("HEAD").exists() {
            std::fs::create_dir_all(path.parent().unwrap_or(&self.root))
                .map_err(|e| GitError::Other(e.to_string()))?;
            let url = format!("{}/{owner}/{name}.git", self.remote_base);
            let dest = path.to_string_lossy();
            run_git(None, &["clone", "--bare", "--filter=blob:none", "--", &url, &dest], token, false)?;
        }
        let missing: Vec<&str> = oids.iter().copied().filter(|o| !has(o)).collect();
        if !missing.is_empty() {
            // Newer commits on branches first, then the commits by id, then PR heads.
            let mut args = vec!["fetch", "--quiet", "origin"];
            args.extend(&missing);
            let by_id = run_git(Some(&path), &args, token, false);
            if by_id.is_err() || missing.iter().any(|o| !has(o)) {
                let pulls = run_git(
                    Some(&path),
                    &["fetch", "--quiet", "origin", "+refs/pull/*/head:refs/pull/*/head"],
                    token,
                    false,
                );
                match (pulls, missing.iter().any(|o| !has(o))) {
                    (Err(GitError::Unauthorized), true) => return Err(GitError::Unauthorized),
                    (Err(e), true) => return Err(GitError::Missing(e.to_string())),
                    _ => {}
                }
            }
            if let Some(o) = missing.iter().find(|o| !has(o)) {
                return Err(GitError::Missing(format!("{o} not found on the remote")));
            }
        }
        // Prefetch blobs: diffing each oid against the merge-base pulls them in.
        for pair in oids.windows(2) {
            let mb = self.merge_base(&path, pair[0], pair[1])?;
            for oid in pair {
                run_git(Some(&path), &["diff", "--numstat", "-z", "-M", &mb, oid], token, false)?;
            }
        }
        Ok(path)
    }

    fn merge_base(&self, clone: &Path, a: &str, b: &str) -> GitResult<String> {
        let out = run_git(Some(clone), &["merge-base", a, b], None, true)?;
        Ok(String::from_utf8_lossy(&out).trim().to_owned())
    }

    /// Changed files between `merge-base(base, head)` and `head`.
    pub fn diff_summary(&self, clone: &Path, base: &str, head: &str) -> GitResult<DiffSummary> {
        let (mb, files) = self.summary_files(clone, base, head)?;
        Ok(DiffSummary {
            merge_base_oid: mb,
            head_oid: head.to_owned(),
            files,
        })
    }

    fn summary_files(&self, clone: &Path, base: &str, head: &str) -> GitResult<(String, Vec<DiffFileSummary>)> {
        if !is_oid(base) || !is_oid(head) {
            return Err(GitError::Other("invalid object id".into()));
        }
        let mb = self.merge_base(clone, base, head)?;
        let raw = run_git(Some(clone), &["diff", "--raw", "-z", "-M", "--no-color", "--no-ext-diff", &mb, head], None, true)?;
        let numstat = run_git(Some(clone), &["diff", "--numstat", "-z", "-M", "--no-color", "--no-ext-diff", &mb, head], None, true)?;
        Ok((mb, parse::summary(&raw, &numstat)))
    }

    /// Hunks for one file in the same range as `diff_summary`.
    pub fn diff_file(&self, clone: &Path, base: &str, head: &str, path: &str) -> GitResult<DiffFile> {
        let (mb, files) = self.summary_files(clone, base, head)?;
        let summary = files
            .into_iter()
            .find(|f| f.path == path)
            .ok_or_else(|| GitError::Other(format!("{path} is not changed in this diff")))?;
        if summary.binary {
            return Ok(DiffFile { summary, hunks: Vec::new() });
        }
        // Both paths, so renames diff as renames.
        let mut args = vec!["diff", "--no-color", "--no-ext-diff", "-M", "-U3", &mb, head, "--"];
        if let Some(old) = &summary.old_path {
            args.push(old);
        }
        args.push(path);
        let out = run_git(Some(clone), &args, None, true)?;
        Ok(DiffFile {
            summary,
            hunks: parse::hunks(&out),
        })
    }
}
