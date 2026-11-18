import type { CommentRow, TimelineEvent } from "../../bindings";

export type TimelineItem =
  | { type: "comment"; at: number; key: string; comment: CommentRow }
  | { type: "event"; at: number; key: string; event: TimelineEvent };

/** Merge comments and events into one list, oldest first. Comments win ties. */
export function buildTimeline(comments: CommentRow[], events: TimelineEvent[]): TimelineItem[] {
  const items: TimelineItem[] = [
    ...comments.map((comment) => ({ type: "comment" as const, at: comment.created_at, key: `c${comment.id}`, comment })),
    ...events.map((event) => ({ type: "event" as const, at: event.created_at, key: `e${event.id}`, event })),
  ];
  return items.sort((a, b) => a.at - b.at || (a.type === b.type ? 0 : a.type === "comment" ? -1 : 1));
}

/** One-line description of a timeline event (without the actor). */
export function describeEvent(e: TimelineEvent): string {
  // payload_json comes from our own sync; a bad payload only loses the detail.
  let payload: Record<string, unknown> = {};
  try {
    payload = e.payload_json ? (JSON.parse(e.payload_json) as Record<string, unknown>) : {};
  } catch {
    // keep the empty payload
  }
  const detail = (key: string) => (typeof payload[key] === "string" ? ` ${String(payload[key])}` : "");
  switch (e.kind) {
    case "labeled": return `added the${detail("label")} label`;
    case "unlabeled": return `removed the${detail("label")} label`;
    case "assigned": return `assigned${detail("assignee")}`;
    case "unassigned": return `unassigned${detail("assignee")}`;
    case "referenced": return `referenced this in${detail("ref")}`;
    case "renamed": return `renamed this from${detail("from")} to${detail("to")}`;
    case "merged": return "merged this";
    case "closed": return "closed this";
    case "reopened": return "reopened this";
    default: return e.kind.replaceAll("_", " ");
  }
}
