//! Five PRs in `forkline-demo/project-001` with real git history, so diffs
//! work offline. Builds a work tree in a temp dir with the `git` CLI, then
//! clones it bare to where `fl_git::GitStore` looks
//! (`<data-dir>/git/<owner>/<name>.git`) and points the PRs at its commits.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use fl_db::rusqlite::{self, Transaction, params};

const OWNER: &str = "forkline-demo";
const NAME: &str = "project-001";
const REPO_ID: i64 = 1;
/// Lines in `src/generated/big.rs`; the diff-scroll bench opens this file.
pub const BIG_LINES: usize = 10_000;

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .current_dir(dir)
        .args(["-c", "core.autocrlf=false", "-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_AUTHOR_NAME", "Mona")
        .env("GIT_AUTHOR_EMAIL", "mona@example.com")
        .env("GIT_COMMITTER_NAME", "Mona")
        .env("GIT_COMMITTER_EMAIL", "mona@example.com")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
        .output()
        .with_context(|| format!("run git {args:?} (Git for Windows is required)"))?;
    if !out.status.success() {
        bail!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn write(dir: &Path, path: &str, text: &str) -> Result<()> {
    let p = dir.join(path);
    std::fs::create_dir_all(p.parent().context("path has a parent")?)?;
    Ok(std::fs::write(p, text)?)
}

fn rust_lines(n: usize, edit_every: Option<usize>) -> String {
    let mut s = String::new();
    for i in 0..n {
        match edit_every {
            Some(k) if i % k == 0 => writeln!(s, "pub const VALUE_{i}: u64 = {} * 2; // tuned", i),
            _ => writeln!(s, "pub const VALUE_{i}: u64 = {i};"),
        }
        .expect("write to String");
    }
    s
}

const LIB_RS: &str = "//! Demo crate.\n\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}\n";

/// Commit on a new branch from `main`, return the head oid.
fn branch(dir: &Path, name: &str, edit: impl FnOnce(&Path) -> Result<()>) -> Result<String> {
    git(dir, &["checkout", "-q", "-b", name, "main"])?;
    edit(dir)?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-q", "-m", name])?;
    git(dir, &["rev-parse", "HEAD"])
}

/// The built repository: where it is, the base commit, and one head per PR.
pub struct Fixture {
    bare: PathBuf,
    base: String,
    heads: Vec<(&'static str, String)>,
}

/// Build the repository on disk. Runs before the database transaction.
pub fn build(data_dir: &Path) -> Result<Fixture> {
    let work = tempfile::tempdir()?;
    let dir = work.path();
    git(dir, &["init", "-q", "-b", "main"])?;
    write(dir, "README.md", "# project-001\n\nGenerated fixture repository.\n")?;
    write(dir, "src/lib.rs", LIB_RS)?;
    write(dir, "src/old_util.rs", "pub fn old() {}\n")?;
    write(dir, "web/app.ts", "export function sum(xs: number[]): number {\n  return xs.reduce((a, b) => a + b, 0);\n}\n")?;
    write(dir, "src/generated/big.rs", &rust_lines(BIG_LINES, None))?;
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-q", "-m", "Initial commit"])?;
    let base = git(dir, &["rev-parse", "HEAD"])?;

    let heads = vec![
        ("fix-add-overflow", branch(dir, "fix-add-overflow", |d| {
            write(d, "src/lib.rs", &LIB_RS.replace("    a + b\n", "    a.saturating_add(b)\n"))
        })?),
        ("web-and-docs", branch(dir, "web-and-docs", |d| {
            write(d, "web/app.ts", "export function sum(xs: number[]): number {\n  let total = 0;\n  for (const x of xs) total += x;\n  return total;\n}\n\nexport const mean = (xs: number[]) => sum(xs) / xs.length;\n")?;
            write(d, "README.md", "# project-001\n\nGenerated fixture repository.\n\n## Usage\n\nRun `cargo test`.\n")
        })?),
        ("rename-util", branch(dir, "rename-util", |d| {
            std::fs::remove_file(d.join("src/old_util.rs"))?;
            write(d, "src/util.rs", "pub fn old() {}\n\npub fn new() {}\n")
        })?),
        ("tune-generated", branch(dir, "tune-generated", |d| {
            write(d, "src/generated/big.rs", &rust_lines(BIG_LINES, Some(10)))
        })?),
        ("python-tools", branch(dir, "python-tools", |d| {
            write(d, "tools/check.py", "import sys\n\n\ndef main() -> int:\n    print(\"ok\")\n    return 0\n\n\nif __name__ == \"__main__\":\n    sys.exit(main())\n")
        })?),
    ];

    let bare = std::path::absolute(data_dir)?.join("git").join(OWNER).join(format!("{NAME}.git"));
    if bare.exists() {
        std::fs::remove_dir_all(&bare)?;
    }
    std::fs::create_dir_all(bare.parent().context("bare path has a parent")?)?;
    git(dir, &["clone", "-q", "--bare", ".", &bare.to_string_lossy()])?;
    Ok(Fixture { bare, base, heads })
}

/// Point the first five open PRs of `project-001` at the fixture's commits.
pub fn seed(tx: &Transaction<'_>, fx: &Fixture) -> rusqlite::Result<()> {
    tx.execute("UPDATE repos SET clone_path = ?2 WHERE id = ?1", params![REPO_ID, fx.bare.to_string_lossy()])?;

    let prs: Vec<i64> = tx
        .prepare("SELECT id FROM issues WHERE repo_id = ?1 AND is_pr = 1 AND state = 'open' ORDER BY number LIMIT 5")?
        .query_map([REPO_ID], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    for (pr, (head_ref, head)) in prs.iter().zip(&fx.heads) {
        tx.execute(
            "UPDATE pr_details SET base_ref = 'main', head_ref = ?2, base_oid = ?3, head_oid = ?4 WHERE issue_id = ?1",
            params![pr, head_ref, fx.base, head],
        )?;
        tx.execute("UPDATE issues SET title = ?2 WHERE id = ?1", params![pr, format!("Real diff: {head_ref}")])?;
    }
    Ok(())
}
