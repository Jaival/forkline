import { describe, expect, it } from "vitest";
import type { DiffFile, DraftReviewComment, ReviewThread } from "../../bindings";
import { buildRows, flattenTree, lineAnchor } from "./rows";
import { makeFixtureFile } from "./fixture";

const thread = (over: Partial<ReviewThread>): ReviewThread => ({
  id: 1,
  path: "a.rs",
  side: "RIGHT",
  line: 3,
  start_line: null,
  original_line: 3,
  is_resolved: false,
  is_outdated: false,
  pending: false,
  comments: [],
  ...over,
});

const small: DiffFile = {
  summary: { path: "a.rs", old_path: null, status: "modified", additions: 1, deletions: 1, binary: false },
  hunks: [
    {
      old_start: 1,
      old_lines: 3,
      new_start: 1,
      new_lines: 3,
      header: "fn main",
      lines: [
        { kind: "context", old_no: 1, new_no: 1, text: "a" },
        { kind: "del", old_no: 2, new_no: null, text: "b" },
        { kind: "add", old_no: null, new_no: 2, text: "c" },
        { kind: "context", old_no: 3, new_no: 3, text: "d" },
      ],
    },
  ],
};

describe("buildRows", () => {
  it("places threads after the line they anchor to", () => {
    const { rows, unplaced } = buildRows(small, [thread({ line: 2 }), thread({ id: 2, side: "LEFT", line: 2 })], [], null);
    expect(unplaced).toEqual([]);
    expect(rows.map((r) => r.kind)).toEqual(["hunk", "line", "line", "thread", "line", "thread", "line"]);
    // LEFT thread follows the deleted line, RIGHT thread follows the added line.
    expect(rows[3]).toMatchObject({ kind: "thread", thread: { id: 2 } });
    expect(rows[5]).toMatchObject({ kind: "thread", thread: { id: 1 } });
  });

  it("returns outdated and off-diff threads as unplaced and ignores other files", () => {
    const outdated = thread({ id: 5, line: null });
    const far = thread({ id: 6, line: 99 });
    const other = thread({ id: 7, path: "b.rs" });
    const { unplaced } = buildRows(small, [outdated, far, other], [], null);
    expect(unplaced.map((t) => t.id)).toEqual([5, 6]);
  });

  it("adds draft comments and the composer", () => {
    const draft: DraftReviewComment = { path: "a.rs", side: "RIGHT", line: 3, start_line: null, body_md: "x" };
    const { rows } = buildRows(small, [], [draft], { side: "RIGHT", line: 1 });
    expect(rows.map((r) => r.kind)).toEqual(["hunk", "line", "composer", "line", "line", "line", "draft"]);
  });

  it("handles a 10k-line fixture", () => {
    const { rows } = buildRows(makeFixtureFile("big.rs", 10_000), [], [], null);
    expect(rows.length).toBeGreaterThan(10_000);
  });
});

describe("lineAnchor", () => {
  it("anchors deleted lines to the left side", () => {
    expect(lineAnchor({ kind: "del", old_no: 4, new_no: null, text: "" })).toEqual({ side: "LEFT", line: 4 });
    expect(lineAnchor({ kind: "context", old_no: 4, new_no: 6, text: "" })).toEqual({ side: "RIGHT", line: 6 });
  });
});

describe("flattenTree", () => {
  const paths = ["src/b.rs", "README.md", "src/sync/x.rs", "src/a.rs"];
  it("lists dirs first, then files", () => {
    expect(flattenTree(paths, new Set()).map((e) => `${e.depth}${e.kind[0]}:${e.name}`)).toEqual([
      "0d:src",
      "1d:sync",
      "2f:x.rs",
      "1f:a.rs",
      "1f:b.rs",
      "0f:README.md",
    ]);
  });
  it("hides children of collapsed dirs", () => {
    expect(flattenTree(paths, new Set(["src"])).map((e) => e.name)).toEqual(["src", "README.md"]);
  });
});
