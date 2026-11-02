import { useMutation } from "@tanstack/react-query";
import { commands, type Action } from "../../bindings";
import { renderBody } from "../../lib/html";

/** Commands throw `{ kind, message }` (CmdError); show it as one line. */
export function errorText(e: unknown): string {
  if (typeof e === "object" && e !== null && "kind" in e) {
    const { kind, message } = e as { kind: string; message?: string };
    return message ? `${kind}: ${message}` : kind;
  }
  return String(e);
}

/** Queue a write action. `error` stays set so the caller can show it inline. */
export function useEnqueue(onSuccess?: () => void) {
  return useMutation({
    mutationFn: (action: Action) => commands.actionEnqueue(action),
    onSuccess: () => onSuccess?.(),
  });
}

/** Rendered GitHub `bodyHTML`, or markdown (local optimistic text) when no HTML exists. */
export function Body({ html, md }: { html: string | null; md: string | null }) {
  const out = renderBody(html, md);
  if (!out) return <p className="text-muted">No description.</p>;
  return (
    <div
      className="break-words [&_a]:text-accent [&_code]:rounded [&_code]:bg-raised [&_code]:px-1 [&_p]:my-2 [&_pre]:my-2 [&_pre]:overflow-auto [&_pre]:rounded [&_pre]:bg-raised [&_pre]:p-2 [&_ul]:list-disc [&_ul]:pl-5"
      dangerouslySetInnerHTML={{ __html: out }}
    />
  );
}

export function when(ts: number | null): string {
  return ts === null ? "" : new Date(ts * 1000).toLocaleString();
}
