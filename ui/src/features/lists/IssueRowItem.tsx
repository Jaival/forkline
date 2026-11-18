import type { IssueRow, IssueState } from "../../bindings";
import type { ViewRef } from "../../lib/layout";
import { timeAgo } from "./format";

export const ROW_HEIGHT = 40;

const STATE_STYLE: Record<IssueState, string> = {
  open: "text-open",
  closed: "text-closed",
  merged: "text-merged",
};

export const viewFor = (item: Pick<IssueRow, "id" | "is_pr">): ViewRef =>
  ({ kind: item.is_pr ? "pr" : "issue", id: item.id });

/** One row in Home and in the Pull requests / Issues / Review requests lists. */
export function IssueRowItem({ item, onNavigate }: { item: IssueRow; onNavigate: (v: ViewRef) => void }) {
  return (
    <button
      data-issue-id={item.id}
      className="flex h-full w-full items-center gap-3 border-b border-border px-4 text-left hover:bg-raised focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-inset focus-visible:outline-none"
      onClick={() => onNavigate(viewFor(item))}
    >
      <span
        aria-label={item.unread ? "Unread" : "Read"}
        className={`size-2 shrink-0 rounded-full ${item.unread ? "bg-accent" : "bg-transparent"}`}
      />
      <span className={`w-14 shrink-0 text-xs uppercase ${STATE_STYLE[item.state]}`}>
        {item.is_pr ? "PR " : ""}{item.state}
      </span>
      <span className="w-52 shrink-0 truncate text-muted">{item.repo} #{item.number}</span>
      <span className={`min-w-0 flex-1 truncate ${item.unread ? "font-semibold" : ""}`}>{item.title}</span>
      <span className="w-24 shrink-0 truncate text-muted">{item.author}</span>
      <span className="w-10 shrink-0 text-right text-muted">{item.comments > 0 ? `💬 ${item.comments}` : ""}</span>
      <span className="w-10 shrink-0 text-right text-muted">{timeAgo(item.updated_at)}</span>
    </button>
  );
}
