import { expect, it } from "vitest";
import type { InboxRow } from "../../bindings";
import { withGroupHeaders } from "./groups";

const row = (id: string, repo: string): InboxRow => ({
  id, repo, repo_id: 1, subject_type: "Issue", issue_id: 1, is_pr: false,
  title: "t", reason: "mention", unread: true, updated_at: 0,
});

it("adds a header per run of repos", () => {
  const items = withGroupHeaders([row("a", "o/x"), row("b", "o/x"), row("c", "o/y")]);
  expect(items.map((i) => (i.type === "header" ? `# ${i.repo}` : i.row.id))).toEqual(["# o/x", "a", "b", "# o/y", "c"]);
});
