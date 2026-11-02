//! Integration tests on local repos built with the git CLI. No network.

use std::path::Path;
use std::process::Command;

use fl_git::GitStore;
use fl_types::{FileStatus, LineKind};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn write(dir: &Path, name: &str, body: &[u8]) {
    std::fs::write(dir.join(name), body).unwrap();
}

struct Fixture {
    _tmp: tempfile::TempDir,
    src: std::path::PathBuf,
    store: GitStore,
    base: String,
    head: String,
}

/// Remote `o/r.git` with `main` (base) and a feature commit (head) that
/// modifies, adds, renames and deletes files and adds a binary.
fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("remote").join("o").join("r.git");
    std::fs::create_dir_all(&src).unwrap();
    git(&src, &["init", "-q", "-b", "main"]);
    git(&src, &["config", "uploadpack.allowFilter", "true"]);
    git(&src, &["config", "uploadpack.allowAnySHA1InWant", "true"]);
    let lines: String = (1..=20).map(|i| format!("line {i}\n")).collect();
    write(&src, "a.txt", lines.as_bytes());
    write(&src, "old.txt", b"one\ntwo\nthree\nfour\nfive\n");
    write(&src, "gone.txt", b"bye\n");
    git(&src, &["add", "."]);
    git(&src, &["commit", "-q", "-m", "base"]);
    let base = git(&src, &["rev-parse", "HEAD"]);

    git(&src, &["checkout", "-q", "-b", "feature"]);
    write(&src, "a.txt", lines.replace("line 10\n", "LINE TEN\nextra\n").as_bytes());
    git(&src, &["mv", "old.txt", "new.txt"]);
    git(&src, &["rm", "-q", "gone.txt"]);
    write(&src, "added.txt", b"fresh\n");
    write(&src, "img.bin", &[0, 1, 2, 0, 255, 0]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-q", "-m", "feature"]);
    let head = git(&src, &["rev-parse", "HEAD"]);
    // Advance main after the branch point, so merge-base != base.
    git(&src, &["checkout", "-q", "main"]);
    write(&src, "main-only.txt", b"x\n");
    git(&src, &["add", "."]);
    git(&src, &["commit", "-q", "-m", "main moves"]);
    let main_tip = git(&src, &["rev-parse", "HEAD"]);

    let url = format!("file:///{}", tmp.path().join("remote").to_string_lossy().replace('\\', "/"));
    let store = GitStore::new(&tmp.path().join("data")).with_remote_base(url);
    let _ = base;
    Fixture { _tmp: tmp, src, store, base: main_tip, head }
}

#[test]
fn clones_blobless_and_diffs_offline() {
    let f = fixture();
    let clone = f.store.ensure_commits("o", "r", Some("secret-token"), &[&f.base, &f.head]).unwrap();
    assert!(clone.ends_with("o/r.git") || clone.ends_with("o\\r.git"));
    // The clone is blobless and its config has no token.
    assert_eq!(git(&clone, &["config", "remote.origin.partialclonefilter"]), "blob:none");
    let config = std::fs::read_to_string(clone.join("config")).unwrap();
    assert!(!config.contains("secret-token"));

    let s = f.store.diff_summary(&clone, &f.base, &f.head).unwrap();
    assert_ne!(s.merge_base_oid, f.base, "merge-base is the branch point, not the main tip");
    let by = |p: &str| s.files.iter().find(|x| x.path == p).unwrap_or_else(|| panic!("{p}: {:?}", s.files));
    assert_eq!(by("a.txt").status, FileStatus::Modified);
    assert_eq!((by("a.txt").additions, by("a.txt").deletions), (2, 1));
    assert_eq!(by("new.txt").status, FileStatus::Renamed);
    assert_eq!(by("new.txt").old_path.as_deref(), Some("old.txt"));
    assert_eq!(by("gone.txt").status, FileStatus::Deleted);
    assert_eq!(by("added.txt").status, FileStatus::Added);
    assert!(by("img.bin").binary);
    assert!(s.files.iter().all(|x| x.path != "main-only.txt"));

    let d = f.store.diff_file(&clone, &f.base, &f.head, "a.txt").unwrap();
    assert_eq!(d.hunks.len(), 1);
    let h = &d.hunks[0];
    assert_eq!((h.old_start, h.new_start), (7, 7));
    let del = h.lines.iter().find(|l| l.kind == LineKind::Del).unwrap();
    assert_eq!((del.old_no, del.new_no, del.text.as_str()), (Some(10), None, "line 10"));
    let add = h.lines.iter().find(|l| l.text == "extra").unwrap();
    assert_eq!((add.old_no, add.new_no), (None, Some(11)));

    let renamed = f.store.diff_file(&clone, &f.base, &f.head, "new.txt").unwrap();
    assert!(renamed.hunks.is_empty(), "pure rename has no hunks");
    assert!(f.store.diff_file(&clone, &f.base, &f.head, "img.bin").unwrap().hunks.is_empty());
    assert!(f.store.diff_file(&clone, &f.base, &f.head, "nope.txt").is_err());
}

#[test]
fn fetches_new_commits_by_id() {
    let f = fixture();
    f.store.ensure_commits("o", "r", None, &[&f.base]).unwrap();
    git(&f.src, &["checkout", "-q", "-b", "later"]);
    write(&f.src, "later.txt", b"later\n");
    git(&f.src, &["add", "."]);
    git(&f.src, &["commit", "-q", "-m", "later"]);
    let tip = git(&f.src, &["rev-parse", "HEAD"]);
    let clone = f.store.ensure_commits("o", "r", None, &[&f.base, &tip]).unwrap();
    let s = f.store.diff_summary(&clone, &f.base, &tip).unwrap();
    assert_eq!(s.files.len(), 1);
    assert_eq!(s.files[0].path, "later.txt");
}

#[test]
fn rejects_bad_input_and_unknown_commits() {
    let f = fixture();
    assert!(f.store.ensure_commits("..", "r", None, &[]).is_err());
    assert!(f.store.ensure_commits("o", "r", None, &["--upload-pack=x"]).is_err());
    let err = f
        .store
        .ensure_commits("o", "r", Some("secret-token"), &["deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"])
        .unwrap_err();
    assert!(matches!(err, fl_git::GitError::Missing(_)), "{err}");
    assert!(!err.to_string().contains("secret-token"));
}
