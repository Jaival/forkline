// Dev/test fixture: generates large diffs so the viewer can be built and
// benchmarked without a git clone. Deterministic; no randomness.
import type { DiffFile, DiffHunk, DiffLine, DiffSummary } from "../../bindings";

/** One file with about `lineCount` diff lines in hunks of ~60 lines. */
export function makeFixtureFile(path: string, lineCount: number): DiffFile {
  const hunks: DiffHunk[] = [];
  let oldNo = 1;
  let newNo = 1;
  let additions = 0;
  let deletions = 0;
  let made = 0;
  while (made < lineCount) {
    const lines: DiffLine[] = [];
    const oldStart = oldNo;
    const newStart = newNo;
    for (let i = 0; i < 60 && made < lineCount; i++, made++) {
      const text = `    let value_${made} = compute(${made}, "${path}"); // fixture line ${made}`;
      const m = made % 7;
      if (m === 3) {
        lines.push({ kind: "del", old_no: oldNo++, new_no: null, text });
        deletions++;
      } else if (m === 4 || m === 5) {
        lines.push({ kind: "add", old_no: null, new_no: newNo++, text });
        additions++;
      } else {
        lines.push({ kind: "context", old_no: oldNo++, new_no: newNo++, text });
      }
    }
    hunks.push({
      old_start: oldStart,
      old_lines: oldNo - oldStart,
      new_start: newStart,
      new_lines: newNo - newStart,
      header: "fn fixture()",
      lines,
    });
    oldNo += 40;
    newNo += 40;
  }
  return { summary: { path, old_path: null, status: "modified", additions, deletions, binary: false }, hunks };
}

const FIXTURE_PATHS: Array<[string, number]> = [
  ["src/lib.rs", 160],
  ["src/sync/engine.rs", 240],
  ["src/db/queries.rs", 120],
  ["ui/src/App.tsx", 90],
  ["docs/ARCHITECTURE.md", 40],
  ["Cargo.toml", 12],
  ["src/generated/big.rs", 10_000],
];

export function fixtureSummary(): DiffSummary {
  return {
    merge_base_oid: "0".repeat(40),
    head_oid: "1".repeat(40),
    files: FIXTURE_PATHS.map(([path, n]) => makeFixtureFile(path, n).summary),
  };
}

export function fixtureFile(path: string): DiffFile {
  const entry = FIXTURE_PATHS.find(([p]) => p === path);
  if (!entry) throw new Error(`no fixture for ${path}`);
  return makeFixtureFile(entry[0], entry[1]);
}
