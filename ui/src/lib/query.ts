import { MutationCache, QueryCache, QueryClient } from "@tanstack/react-query";
import { events } from "../bindings";

const signInListeners = new Set<() => void>();

/** Run `f` when any command fails with `CmdError::Unauthorized`. */
export function onSignInNeeded(f: () => void): () => void {
  signInListeners.add(f);
  return () => signInListeners.delete(f);
}

/** Shared handler for `CmdError::Unauthorized` from any query or mutation. */
export function noteError(e: unknown) {
  if (typeof e !== "object" || e === null || (e as { kind?: unknown }).kind !== "Unauthorized") return;
  void queryClient.invalidateQueries({ queryKey: ["sync"] });
  for (const f of signInListeners) f();
}

// All data is local, so queries never go stale on their own. They refresh
// only when the backend emits a "data changed" event for their scope.
export const queryClient = new QueryClient({
  queryCache: new QueryCache({ onError: noteError }),
  mutationCache: new MutationCache({ onError: noteError }),
  defaultOptions: {
    queries: {
      staleTime: Infinity,
      gcTime: 5 * 60_000,
      retry: false,
      refetchOnWindowFocus: false,
      networkMode: "always",
    },
    mutations: { networkMode: "always" },
  },
});

/**
 * Query keys start with a scope that matches `DataChanged.scopes` on the
 * Rust side, e.g. ["issues", "home"], ["repos", "pinned"], ["sync"].
 */
export function listenForDataChanges(): Promise<() => void> {
  return events.dataChangedEvent.listen(({ payload }) => {
    const scopes = new Set(payload.scopes);
    void queryClient.invalidateQueries({ predicate: (q) => scopes.has(String(q.queryKey[0])) });
  });
}
