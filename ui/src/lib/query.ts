import { QueryClient } from "@tanstack/react-query";
import { events } from "../bindings";

// All data is local, so queries never go stale on their own. They refresh
// only when the backend emits a "data changed" event for their scope.
export const queryClient = new QueryClient({
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
