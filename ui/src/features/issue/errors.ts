/** Turn a thrown `CmdError` (`{ kind, message }`) into a line for the UI. */
export function formatError(e: unknown): string {
  if (typeof e === "object" && e !== null && "kind" in e) {
    const { kind, message } = e as { kind: string; message?: string };
    if (kind === "NotImplemented") return "This action cannot be sent yet: the action queue is not available.";
    if (kind === "Unauthorized") return "Not signed in.";
    return message ? `${kind}: ${message}` : kind;
  }
  return e instanceof Error ? e.message : String(e);
}
