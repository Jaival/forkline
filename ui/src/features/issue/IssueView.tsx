import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { commands, type IssueState } from "../../bindings";
import type { ViewRef } from "../../lib/layout";
import { signalReady } from "../../lib/perf";
import { timeAgo } from "../lists/format";
import { viewFor } from "../lists/IssueRowItem";
import { Composer } from "./Composer";
import { Markdown } from "./Markdown";
import { buildTimeline, describeEvent } from "./timeline";
import { useAction } from "./useAction";

const STATE_STYLE: Record<IssueState, string> = {
  open: "bg-open",
  closed: "bg-closed",
  merged: "bg-merged",
};

const PENDING = <span className="ml-2 rounded bg-raised px-1.5 text-xs text-muted">pending</span>;

const toggle = (ids: number[], id: number) => (ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id]);

/** Issue (and, until the PR view lands, PR) detail from the local cache. */
export function IssueView({ id, onNavigate }: { id: number; onNavigate: (v: ViewRef) => void }) {
  const issue = useQuery({
    queryKey: ["issues", "detail", id],
    queryFn: () => commands.issueGet(id),
  });
  const thread = useQuery({
    queryKey: ["issues", "thread", id],
    queryFn: () => commands.issueThread(id),
  });
  const repoId = issue.data?.row.repo_id;
  const options = useQuery({
    queryKey: ["issues", "options", repoId],
    queryFn: () => commands.issueEditOptions(repoId!),
    enabled: repoId !== undefined,
  });
  const action = useAction();
  const [editing, setEditing] = useState<"labels" | "assignees" | null>(null);

  const ready = issue.isSuccess && thread.isSuccess;
  useEffect(() => {
    if (ready) signalReady(`issue:${id}`);
  }, [ready, id]);

  if (issue.isPending || thread.isPending) return <div className="p-6 text-muted">Loading cached item…</div>;
  if (issue.isError || thread.isError) {
    return <div className="p-6" role="alert">Could not load this item from the local cache.</div>;
  }
  const { row, labels } = issue.data;
  const t = thread.data;
  const pending = new Set(t.pending_fields);
  const timeline = buildTimeline(t.comments, t.timeline);
  const labelIds = labels.map((l) => l.id);
  const userIds = t.assignees.map((u) => u.id);

  return (
    <article className="h-full overflow-auto">
      <div className="mx-auto max-w-4xl space-y-5 p-6">
        <header>
          <p className="mb-1 text-sm text-muted">{row.repo} #{row.number}</p>
          <h1 className="text-xl font-semibold">{row.title}</h1>
          <p className="mt-2 flex flex-wrap items-center gap-2 text-sm text-muted">
            <span className={`rounded-full px-2 py-0.5 text-white ${STATE_STYLE[row.state]}`}>{row.state}</span>
            {pending.has("state") ? PENDING : null}
            <span>{row.author} opened {timeAgo(issue.data.created_at)} ago · {row.comments} comments</span>
          </p>
        </header>

        <section aria-label="Details" className="flex flex-wrap items-start gap-x-8 gap-y-3 text-sm">
          <div>
            <span className="text-muted">Labels</span>
            {pending.has("labels") ? PENDING : null}
            <div className="mt-1 flex flex-wrap items-center gap-1">
              {labels.length === 0 ? <span className="text-muted">None</span> : null}
              {labels.map((l) => (
                <span key={l.id} className="rounded-full border px-2" style={{ borderColor: `#${l.color}`, color: `#${l.color}` }}>
                  {l.name}
                </span>
              ))}
              <button className="rounded px-1 text-accent" onClick={() => setEditing(editing === "labels" ? null : "labels")}>
                Edit
              </button>
            </div>
          </div>
          <div>
            <span className="text-muted">Assignees</span>
            {pending.has("assignees") ? PENDING : null}
            <div className="mt-1 flex flex-wrap items-center gap-2">
              {t.assignees.length === 0 ? <span className="text-muted">None</span> : null}
              {t.assignees.map((u) => <span key={u.id}>{u.login}</span>)}
              <button className="rounded px-1 text-accent" onClick={() => setEditing(editing === "assignees" ? null : "assignees")}>
                Edit
              </button>
            </div>
          </div>
          {t.linked.length > 0 ? (
            <div>
              <span className="text-muted">{row.is_pr ? "Closes" : "Linked pull requests"}</span>
              <div className="mt-1 flex flex-wrap gap-2">
                {t.linked.map((l) => (
                  <button key={l.id} className="rounded border border-border px-2 hover:bg-raised" onClick={() => onNavigate(viewFor(l))}>
                    #{l.number} {l.title}
                  </button>
                ))}
              </div>
            </div>
          ) : null}
        </section>

        {editing === "labels" ? (
          <fieldset className="flex flex-wrap gap-x-5 gap-y-1 rounded border border-border p-3" aria-label="Edit labels">
            {options.data?.labels.map((l) => (
              <label key={l.id} className="flex items-center gap-1">
                <input
                  type="checkbox"
                  checked={labelIds.includes(l.id)}
                  disabled={action.busy}
                  onChange={() => void action.run({ kind: "set_labels", issue_id: id, label_ids: toggle(labelIds, l.id) })}
                />
                {l.name}
              </label>
            ))}
          </fieldset>
        ) : null}
        {editing === "assignees" ? (
          <fieldset className="flex flex-wrap gap-x-5 gap-y-1 rounded border border-border p-3" aria-label="Edit assignees">
            {options.data?.users.map((u) => (
              <label key={u.id} className="flex items-center gap-1">
                <input
                  type="checkbox"
                  checked={userIds.includes(u.id)}
                  disabled={action.busy}
                  onChange={() => void action.run({ kind: "set_assignees", issue_id: id, user_ids: toggle(userIds, u.id) })}
                />
                {u.login}
              </label>
            ))}
          </fieldset>
        ) : null}

        <section className="rounded border border-border p-4" aria-label="Description">
          <Markdown html={issue.data.body_html} md={issue.data.body_md} />
        </section>

        <ol className="space-y-3">
          {timeline.map((item) =>
            item.type === "comment" ? (
              <li key={item.key} className="rounded border border-border">
                <div className="flex items-center gap-2 border-b border-border bg-panel px-3 py-1.5 text-sm text-muted">
                  <strong className="text-fg">{item.comment.author}</strong>
                  <span>{timeAgo(item.comment.created_at)} ago</span>
                  {item.comment.id < 0 ? <span className="rounded bg-raised px-1.5 text-xs">sending…</span> : null}
                </div>
                <div className="p-3">
                  <Markdown html={item.comment.body_html} md={item.comment.body_md} />
                </div>
              </li>
            ) : (
              <li key={item.key} className="px-3 text-sm text-muted">
                <strong className="text-fg">{item.event.actor}</strong> {describeEvent(item.event)} · {timeAgo(item.event.created_at)} ago
              </li>
            ),
          )}
        </ol>

        <Composer disabled={action.busy} onSubmit={(body_md) => action.run({ kind: "comment", issue_id: id, body_md })} />
        <div className="flex items-center gap-3">
          {row.state !== "merged" ? (
            <button
              className="rounded border border-border px-3 py-1 hover:bg-raised disabled:opacity-50"
              disabled={action.busy || pending.has("state")}
              onClick={() => void action.run({ kind: "set_state", issue_id: id, state: row.state === "open" ? "closed" : "open" })}
            >
              {row.state === "open" ? "Close" : "Reopen"}
            </button>
          ) : null}
          {action.error ? <p role="alert" className="text-closed">{action.error}</p> : null}
        </div>
      </div>
    </article>
  );
}
