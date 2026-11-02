import { useState } from "react";
import { type Action, commands } from "../../bindings";
import { noteError } from "../../lib/query";
import { formatError } from "./errors";

/**
 * Send a write action to the queue. The queue applies it optimistically and
 * emits `data-changed`, so there is nothing to update here. `error` holds the
 * last failure (e.g. the queue is not available yet) for inline display.
 */
export function useAction() {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const run = async (action: Action): Promise<boolean> => {
    setBusy(true);
    try {
      await commands.actionEnqueue(action);
      setError(null);
      return true;
    } catch (e) {
      noteError(e);
      setError(formatError(e));
      return false;
    } finally {
      setBusy(false);
    }
  };
  return { run, error, busy };
}
