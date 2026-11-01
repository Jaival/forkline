import { useQuery } from "@tanstack/react-query";
import { commands } from "../../bindings";
import type { ViewRef } from "../../lib/layout";

export function HomeView({ onNavigate }: { onNavigate: (view: ViewRef) => void }) {
  const feed = useQuery({
    queryKey: ["issues", "home", 0],
    queryFn: () => commands.homeFeed({ offset: 0, limit: 50 }),
  });

  return (
    <div className="h-full overflow-auto p-6">
      <h1 className="mb-4 text-xl font-semibold">Home</h1>
      {feed.isPending ? <p className="text-muted">Loading cached activity…</p> : null}
      {feed.isError ? <p role="alert">Could not read the local activity cache.</p> : null}
      {feed.data?.items.length === 0 ? <p className="text-muted">No cached activity yet.</p> : null}
      <ul className="divide-y divide-border">
        {feed.data?.items.map((item) => (
          <li key={item.id}>
            <button
              className="block w-full py-3 text-left hover:bg-raised"
              onClick={() => onNavigate({ kind: item.is_pr ? "pr" : "issue", id: item.id })}
            >
              <span className="text-muted">{item.repo} #{item.number}</span>
              <span className="ml-2">{item.title}</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
