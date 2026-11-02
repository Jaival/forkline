// Pure helpers for the sign-in dialog (unit tested).
import type { AuthPoll } from "../../bindings";

export type PollStep =
  | { kind: "wait"; ms: number }
  | { kind: "done"; login: string }
  | { kind: "stop"; message: string };

/** What to do after one `auth_poll` answer. */
export function nextStep(p: AuthPoll): PollStep {
  switch (p.status) {
    case "pending":
      return { kind: "wait", ms: Math.max(1, p.interval) * 1000 };
    case "done":
      return { kind: "done", login: p.login };
    case "expired":
      return { kind: "stop", message: "The code expired. Start again." };
    case "denied":
      return { kind: "stop", message: "Sign-in was cancelled on GitHub." };
  }
}

/** Text of a command error (`CmdError` is `{ kind, message? }`). */
export function errorText(e: unknown): string {
  if (typeof e === "object" && e !== null) {
    const o = e as { kind?: unknown; message?: unknown };
    if (typeof o.message === "string") return o.message;
    if (o.kind === "Unauthorized") return "Not signed in.";
    if (typeof o.kind === "string") return o.kind;
  }
  return String(e);
}
