import { useState } from "react";
import type { DraftReviewComment, MergeMethod, PrDetail, ReviewEvent } from "../../bindings";
import { errorText, useEnqueue } from "./shared";

const EVENTS: Array<[ReviewEvent, string]> = [
  ["COMMENT", "Comment"],
  ["APPROVE", "Approve"],
  ["REQUEST_CHANGES", "Request changes"],
];

/** Submit a review with the inline comments drafted in the Files tab. */
export function ReviewForm({
  prId,
  drafts,
  onSubmitted,
}: {
  prId: number;
  drafts: DraftReviewComment[];
  onSubmitted: () => void;
}) {
  const [event, setEvent] = useState<ReviewEvent>("COMMENT");
  const [body, setBody] = useState("");
  const submit = useEnqueue(() => {
    setBody("");
    onSubmitted();
  });
  // GitHub rejects a comment / request-changes review with nothing in it.
  const empty = event !== "APPROVE" && body.trim() === "" && drafts.length === 0;
  return (
    <form
      className="rounded border border-border p-4"
      onSubmit={(e) => {
        e.preventDefault();
        submit.mutate({ kind: "submit_review", pr_id: prId, event, body_md: body.trim(), comments: drafts });
      }}
    >
      <h2 className="mb-2 font-semibold">Review</h2>
      <textarea
        className="h-24 w-full resize-y rounded border border-border bg-bg p-2"
        placeholder="Leave a review comment"
        value={body}
        onChange={(e) => setBody(e.target.value)}
      />
      <div className="mt-2 flex items-center gap-3">
        <select className="rounded border border-border bg-bg px-2 py-1" value={event} onChange={(e) => setEvent(e.target.value as ReviewEvent)}>
          {EVENTS.map(([v, t]) => <option key={v} value={v}>{t}</option>)}
        </select>
        <span className="text-xs text-muted">{drafts.length} pending inline {drafts.length === 1 ? "comment" : "comments"}</span>
        <button className="ml-auto rounded bg-accent px-3 py-1 text-white disabled:opacity-50" disabled={empty || submit.isPending}>
          Submit review
        </button>
      </div>
      {submit.isError ? <p role="alert" className="mt-2 text-xs text-closed">Could not queue the review: {errorText(submit.error)}</p> : null}
    </form>
  );
}

const METHODS: Array<[MergeMethod, string]> = [
  ["merge", "Merge commit"],
  ["squash", "Squash and merge"],
  ["rebase", "Rebase and merge"],
];

export function MergeBox({ d }: { d: PrDetail }) {
  const [method, setMethod] = useState<MergeMethod>("squash");
  const [confirm, setConfirm] = useState(false);
  const merge = useEnqueue(() => setConfirm(false));
  if (d.row.state !== "open") return null;

  const blocked = d.is_draft
    ? "This pull request is a draft."
    : d.mergeable === "CONFLICTING"
      ? "This branch has conflicts that must be resolved."
      : d.pending_fields.length > 0
        ? "A change is waiting to sync."
        : null;
  return (
    <section className="rounded border border-border p-4">
      <h2 className="mb-2 font-semibold">Merge</h2>
      {blocked ? <p className="mb-2 text-sm text-muted">{blocked}</p> : null}
      <div className="flex items-center gap-3">
        <select className="rounded border border-border bg-bg px-2 py-1" value={method} onChange={(e) => { setMethod(e.target.value as MergeMethod); setConfirm(false); }}>
          {METHODS.map(([v, t]) => <option key={v} value={v}>{t}</option>)}
        </select>
        {confirm ? (
          <>
            <button className="rounded bg-open px-3 py-1 text-white" disabled={merge.isPending} onClick={() => merge.mutate({ kind: "merge", pr_id: d.row.id, method })}>
              Confirm {method}
            </button>
            <button className="rounded px-3 py-1 hover:bg-raised" onClick={() => setConfirm(false)}>Cancel</button>
          </>
        ) : (
          <button className="rounded bg-open px-3 py-1 text-white disabled:opacity-50" disabled={blocked !== null} onClick={() => setConfirm(true)}>
            Merge pull request
          </button>
        )}
      </div>
      {merge.isError ? <p role="alert" className="mt-2 text-xs text-closed">Could not queue the merge: {errorText(merge.error)}</p> : null}
    </section>
  );
}
