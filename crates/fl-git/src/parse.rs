//! Parsers for `git diff` output (pure, unit tested).

use fl_types::{DiffFileSummary, DiffHunk, DiffLine, FileStatus, LineKind};

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// `--raw -z -M` gives status and paths; `--numstat -z -M` gives counts.
pub fn summary(raw: &[u8], numstat: &[u8]) -> Vec<DiffFileSummary> {
    let mut files = Vec::new();
    let mut toks = raw.split(|&b| b == 0).filter(|t| !t.is_empty());
    while let Some(meta) = toks.next() {
        // ":100644 100644 <old> <new> M"
        // The last word is the status, with a score for renames ("R100").
        let status_code = meta.rsplit(|&b| b == b' ').next().and_then(|w| w.first().copied()).unwrap_or(b'M');
        let Some(first) = toks.next() else { break };
        let (old_path, path) = if status_code == b'R' || status_code == b'C' {
            let Some(second) = toks.next() else { break };
            (Some(text(first)), text(second))
        } else {
            (None, text(first))
        };
        files.push(DiffFileSummary {
            path,
            old_path,
            status: match status_code {
                b'A' | b'C' => FileStatus::Added,
                b'D' => FileStatus::Deleted,
                b'R' => FileStatus::Renamed,
                _ => FileStatus::Modified,
            },
            additions: 0,
            deletions: 0,
            binary: false,
        });
    }

    // numstat: "add\tdel\tpath\0" or "add\tdel\t\0old\0new\0".
    let mut toks = numstat.split(|&b| b == 0).filter(|t| !t.is_empty());
    while let Some(line) = toks.next() {
        let mut parts = line.splitn(3, |&b| b == b'\t');
        let (Some(a), Some(d), Some(p)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let path = if p.is_empty() {
            toks.next(); // old path
            toks.next().map(text)
        } else {
            Some(text(p))
        };
        let Some(path) = path else { break };
        if let Some(f) = files.iter_mut().find(|f| f.path == path) {
            match (text(a).parse(), text(d).parse()) {
                (Ok(a), Ok(d)) => {
                    f.additions = a;
                    f.deletions = d;
                }
                _ => f.binary = true, // "-\t-"
            }
        }
    }
    files
}

fn range(s: &str) -> Option<(u32, u32)> {
    // "12,5" or "12" (count defaults to 1)
    match s.split_once(',') {
        Some((a, b)) => Some((a.parse().ok()?, b.parse().ok()?)),
        None => Some((s.parse().ok()?, 1)),
    }
}

/// "@@ -a,b +c,d @@ header"
fn hunk_header(line: &str) -> Option<DiffHunk> {
    let rest = line.strip_prefix("@@ -")?;
    let (ranges, header) = rest.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    let (old_start, old_lines) = range(old)?;
    let (new_start, new_lines) = range(new)?;
    Some(DiffHunk {
        old_start,
        old_lines,
        new_start,
        new_lines,
        header: header.trim_start().to_owned(),
        lines: Vec::new(),
    })
}

/// Unified diff of one file (`-U3`). Skips the file header.
pub fn hunks(patch: &[u8]) -> Vec<DiffHunk> {
    let mut out: Vec<DiffHunk> = Vec::new();
    let (mut old_no, mut new_no) = (0, 0);
    for raw in patch.split(|&b| b == b'\n') {
        let line = text(raw);
        if line.starts_with("@@ ") {
            if let Some(h) = hunk_header(&line) {
                (old_no, new_no) = (h.old_start, h.new_start);
                out.push(h);
            }
            continue;
        }
        let Some(h) = out.last_mut() else { continue };
        let (kind, body) = match line.as_bytes().first() {
            Some(b'+') => (LineKind::Add, &line[1..]),
            Some(b'-') => (LineKind::Del, &line[1..]),
            Some(b' ') => (LineKind::Context, &line[1..]),
            _ => continue, // "\ No newline at end of file" or trailing empty
        };
        let (o, n) = match kind {
            LineKind::Add => (None, Some(new_no)),
            LineKind::Del => (Some(old_no), None),
            LineKind::Context => (Some(old_no), Some(new_no)),
        };
        if o.is_some() {
            old_no += 1;
        }
        if n.is_some() {
            new_no += 1;
        }
        h.lines.push(DiffLine {
            kind,
            old_no: o,
            new_no: n,
            text: body.to_owned(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hunks_with_line_numbers() {
        let patch = b"diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n@@ -1,3 +1,4 @@ fn x()\n a\n-b\n+B\n+c\n d\n\\ No newline at end of file\n";
        let h = hunks(patch);
        assert_eq!(h.len(), 1);
        assert_eq!((h[0].old_start, h[0].old_lines, h[0].new_start, h[0].new_lines), (1, 3, 1, 4));
        assert_eq!(h[0].header, "fn x()");
        let l = &h[0].lines;
        assert_eq!(l.len(), 5);
        assert_eq!((l[1].kind, l[1].old_no, l[1].new_no), (LineKind::Del, Some(2), None));
        assert_eq!((l[2].kind, l[2].old_no, l[2].new_no), (LineKind::Add, None, Some(2)));
        assert_eq!((l[4].old_no, l[4].new_no), (Some(3), Some(4)));
    }

    #[test]
    fn parses_single_line_range() {
        let h = hunk_header("@@ -0,0 +1 @@").unwrap();
        assert_eq!((h.old_start, h.old_lines, h.new_start, h.new_lines), (0, 0, 1, 1));
    }

    #[test]
    fn parses_summary_with_rename_and_binary() {
        let raw = b":100644 100644 aaa bbb M\0a.txt\0:100644 100644 aaa bbb R100\0old.txt\0new.txt\0:000000 100644 000 ccc A\0img.bin\0";
        let num = [&b"1\t2\ta.txt\0"[..], &b"0\t0\t\0old.txt\0new.txt\0"[..], &b"-\t-\timg.bin\0"[..]].concat();
        let f = summary(raw, &num);
        assert_eq!(f.len(), 3);
        assert_eq!((f[0].additions, f[0].deletions), (1, 2));
        assert_eq!(f[1].status, FileStatus::Renamed);
        assert_eq!(f[1].old_path.as_deref(), Some("old.txt"));
        assert!(f[2].binary);
    }
}
