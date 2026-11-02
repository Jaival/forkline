import { useState } from "react";
import type { DraftReviewComment, ReviewThread } from "../../bindings";
import { Body, errorText, useEnqueue, when } from "../pr/shared";

/** One review thread: comments, resolve toggle and a reply box. */
export function ThreadCard({ thread }: { thread: ReviewThread }) {
  const [reply, setReply] = useState("");
  const send = useEnqueue(() => setReply(""));
  const resolve = useEnqueue();
  const error = send.error ?? resolve.error;
  return (
    <div className="mx-3 my-1 rounded border border-border bg-panel font-sans text-[13px]">
      <div className="flex items-center gap-2 border-b border-border px-3 py-1 text-xs text-muted">
        <span>{thread.is_resolved ? "Resolved" : "Open thread"}</span>
        {thread.is_outdated ? <span>Outdated</span> : null}
        {thread.pending ? <span>Waiting to sync…</span> : null}
        <button
          className="ml-auto rounded px-2 py-0.5 hover:bg-raised"
          disabled={resolve.isPending}
          onClick={() => resolve.mutate({ kind: "resolve_thread", thread_id: thread.id, resolved: !thread.is_resolved })}
        >
          {thread.is_resolved ? "Unresolve" : "Resolve"}
        </button>
      </div>
      {thread.is_resolved ? null : (
        <>
          {thread.comments.map((c) => (
            <div key={c.id} className="border-b border-border px-3 py-2 last:border-b-0">
              <div className="text-xs text-muted">
                <b className="text-fg">{c.author}</b> · {when(c.created_at)}
              </div>
              <Body html={c.body_html} md={c.body_md} />
            </div>
          ))}
          <form
            className="flex gap-2 border-t border-border p-2"
            onSubmit={(e) => {
              e.preventDefault();
              send.mutate({ kind: "reply_thread", thread_id: thread.id, body_md: reply.trim() });
            }}
          >
            <input
              className="min-w-0 flex-1 rounded border border-border bg-bg px-2 py-1"
              placeholder="Reply…"
              value={reply}
              onChange={(e) => setReply(e.target.value)}
            />
            <button className="rounded bg-accent px-3 py-1 text-white disabled:opacity-50" disabled={!reply.trim() || send.isPending}>
              Reply
            </button>
          </form>
        </>
      )}
      {error ? <p role="alert" className="px-3 pb-2 text-xs text-closed">Could not queue: {errorText(error)}</p> : null}
    </div>
  );
}

/** A comment written in the diff but not submitted yet (part of the pending review). */
export function DraftCard({ draft, onRemove }: { draft: DraftReviewComment; onRemove: () => void }) {
  return (
    <div className="mx-3 my-1 rounded border border-dashed border-border bg-panel px-3 py-2 font-sans text-[13px]">
      <div className="mb-1 flex items-center text-xs text-muted">
        Pending comment (submit it with your review)
        <button className="ml-auto rounded px-2 py-0.5 hover:bg-raised" onClick={onRemove}>
          Remove
        </button>
      </div>
      <p className="whitespace-pre-wrap">{draft.body_md}</p>
    </div>
  );
}

export function Composer({ onSave, onCancel }: { onSave: (body: string) => void; onCancel: () => void }) {
  const [body, setBody] = useState("");
  return (
    <form
      className="mx-3 my-1 rounded border border-border bg-panel p-2 font-sans text-[13px]"
      onSubmit={(e) => {
        e.preventDefault();
        onSave(body.trim());
      }}
    >
      <textarea
        autoFocus
        className="h-20 w-full resize-y rounded border border-border bg-bg p-2"
        placeholder="Leave a comment"
        value={body}
        onChange={(e) => setBody(e.target.value)}
      />
      <div className="mt-1 flex justify-end gap-2">
        <button type="button" className="rounded px-3 py-1 hover:bg-raised" onClick={onCancel}>
          Cancel
        </button>
        <button className="rounded bg-accent px-3 py-1 text-white disabled:opacity-50" disabled={!body.trim()}>
          Add to review
        </button>
      </div>
    </form>
  );
}
