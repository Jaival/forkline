import type { DiffFile, DiffLine, DiffSide, DraftReviewComment, ReviewThread } from "../../bindings";

export type DiffRow =
  | { kind: "hunk"; text: string }
  | { kind: "line"; line: DiffLine }
  | { kind: "thread"; thread: ReviewThread }
  | { kind: "draft"; draft: DraftReviewComment }
  | { kind: "composer"; side: DiffSide; line: number };

export type Composer = { side: DiffSide; line: number } | null;

/** Where a click on a line's gutter would anchor a comment. */
export function lineAnchor(l: DiffLine): { side: DiffSide; line: number } | null {
  if (l.kind === "del") return l.old_no === null ? null : { side: "LEFT", line: l.old_no };
  return l.new_no === null ? null : { side: "RIGHT", line: l.new_no };
}

const key = (side: DiffSide, line: number) => `${side}:${line}`;

function push<T>(m: Map<string, T[]>, k: string, v: T) {
  const list = m.get(k);
  if (list) list.push(v);
  else m.set(k, [v]);
}

/**
 * Flatten one file's hunks into virtual-list rows. Threads, saved draft
 * comments and the open composer are placed right after the line they
 * anchor to. Threads that match no line (outdated, or outside the shown
 * hunks) come back in `unplaced`.
 */
export function buildRows(
  file: DiffFile,
  threads: ReviewThread[],
  drafts: DraftReviewComment[],
  composer: Composer,
): { rows: DiffRow[]; unplaced: ReviewThread[] } {
  const threadAt = new Map<string, ReviewThread[]>();
  const unplaced: ReviewThread[] = [];
  for (const t of threads) {
    if (t.path !== file.summary.path) continue;
    if (t.line === null) unplaced.push(t);
    else push(threadAt, key(t.side, t.line), t);
  }
  const draftAt = new Map<string, DraftReviewComment[]>();
  for (const d of drafts) if (d.path === file.summary.path) push(draftAt, key(d.side, d.line), d);

  const rows: DiffRow[] = [];
  const placed = new Set<ReviewThread>();
  for (const h of file.hunks) {
    rows.push({
      kind: "hunk",
      text: `@@ -${h.old_start},${h.old_lines} +${h.new_start},${h.new_lines} @@ ${h.header}`.trimEnd(),
    });
    for (const line of h.lines) {
      rows.push({ kind: "line", line });
      const anchors = [
        line.old_no === null || line.kind === "add" ? null : key("LEFT", line.old_no),
        line.new_no === null || line.kind === "del" ? null : key("RIGHT", line.new_no),
      ];
      for (const k of anchors) {
        if (k === null) continue;
        for (const t of threadAt.get(k) ?? []) {
          rows.push({ kind: "thread", thread: t });
          placed.add(t);
        }
        for (const d of draftAt.get(k) ?? []) rows.push({ kind: "draft", draft: d });
        if (composer && key(composer.side, composer.line) === k) rows.push({ kind: "composer", ...composer });
      }
    }
  }
  for (const list of threadAt.values()) for (const t of list) if (!placed.has(t)) unplaced.push(t);
  return { rows, unplaced };
}

export type TreeEntry = { kind: "dir" | "file"; path: string; name: string; depth: number };

/** Directory tree of file paths, flattened in display order. Collapsed dirs hide their children. */
export function flattenTree(paths: string[], collapsed: ReadonlySet<string>): TreeEntry[] {
  type Node = { dirs: Map<string, Node>; files: string[] };
  const root: Node = { dirs: new Map(), files: [] };
  for (const p of paths) {
    const parts = p.split("/");
    let node = root;
    for (const part of parts.slice(0, -1)) {
      let next = node.dirs.get(part);
      if (!next) node.dirs.set(part, (next = { dirs: new Map(), files: [] }));
      node = next;
    }
    node.files.push(p);
  }
  const out: TreeEntry[] = [];
  const walk = (node: Node, prefix: string, depth: number) => {
    for (const [name, child] of [...node.dirs].sort((a, b) => a[0].localeCompare(b[0]))) {
      const path = prefix + name;
      out.push({ kind: "dir", path, name, depth });
      if (!collapsed.has(path)) walk(child, path + "/", depth + 1);
    }
    for (const p of node.files.sort()) {
      out.push({ kind: "file", path: p, name: p.slice(p.lastIndexOf("/") + 1), depth });
    }
  };
  walk(root, "", 0);
  return out;
}
