import { useEffect } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { commands } from "../../bindings";
import type { ViewRef } from "../../lib/layout";
import { signalReady } from "../../lib/perf";
import { splitHighlight } from "./highlight";

const PAGE = 50;

export function SearchView({ q, onNavigate }: { q: string; onNavigate: (view: ViewRef) => void }) {
  const results = useInfiniteQuery({
    queryKey: ["search", q],
    queryFn: ({ pageParam }) => commands.search(q, { offset: pageParam, limit: PAGE }),
    initialPageParam: 0,
    getNextPageParam: (last) => last.next ?? undefined,
  });

  // Rows (or the empty state) are committed once this effect runs.
  useEffect(() => {
    if (results.isSuccess) signalReady(`search:${q}`);
  }, [results.isSuccess, q]);

  const hits = results.data?.pages.flatMap((p) => p.items) ?? [];

  return (
    <div className="h-full overflow-auto p-6">
      <form
        className="mb-4"
        onSubmit={(e) => {
          e.preventDefault();
          const text = new FormData(e.currentTarget).get("q");
          if (typeof text === "string") onNavigate({ kind: "search", q: text.trim() });
        }}
      >
        <input
          id="search-view-input"
          className="w-full rounded border border-border bg-raised px-3 py-2"
          key={q}
          name="q"
          defaultValue={q}
          placeholder="Search issues and pull requests. Try is:pr author:name label:&quot;bug&quot; repo:owner/name -is:closed"
          aria-label="Search"
        />
      </form>
      {results.isPending ? <p className="text-muted">Searching…</p> : null}
      {results.isError ? <p role="alert">Search failed.</p> : null}
      {results.isSuccess && hits.length === 0 ? <p className="text-muted">No results.</p> : null}
      <ul className="divide-y divide-border">
        {hits.map((hit) => (
          <li key={hit.row.id}>
            <button
              className="block w-full py-3 text-left hover:bg-raised"
              onClick={() => onNavigate({ kind: hit.row.is_pr ? "pr" : "issue", id: hit.row.id })}
            >
              <span className="text-muted">
                {hit.row.repo} #{hit.row.number} · {hit.row.is_pr ? "PR" : "issue"} · {hit.row.state}
              </span>
              <span className="ml-2">{hit.row.title}</span>
              {hit.snippet ? (
                <span className="mt-1 block truncate text-sm text-muted">
                  {hit.comment_id !== null ? "In a comment: " : null}
                  {splitHighlight(hit.snippet).map((s, i) =>
                    s.hit ? (
                      <mark key={i} className="bg-transparent font-semibold text-fg">
                        {s.text}
                      </mark>
                    ) : (
                      s.text
                    ),
                  )}
                </span>
              ) : null}
            </button>
          </li>
        ))}
      </ul>
      {results.hasNextPage ? (
        <button
          className="mt-4 rounded border border-border px-3 py-1 hover:bg-raised"
          disabled={results.isFetchingNextPage}
          onClick={() => void results.fetchNextPage()}
        >
          Load more
        </button>
      ) : null}
    </div>
  );
}
