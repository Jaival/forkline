import type { InboxRow } from "../../bindings";

export type InboxItem = { type: "header"; repo: string } | { type: "row"; row: InboxRow };

/** Insert a repo header before each run of rows from the same repo. */
export function withGroupHeaders(rows: InboxRow[]): InboxItem[] {
  const out: InboxItem[] = [];
  let repo: string | null = null;
  for (const row of rows) {
    if (row.repo !== repo) {
      repo = row.repo;
      out.push({ type: "header", repo });
    }
    out.push({ type: "row", row });
  }
  return out;
}
