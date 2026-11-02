import { useEffect, useRef, useState } from "react";
import { useInfiniteQuery, useMutation } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { commands } from "../../bindings";
import type { ViewRef } from "../../lib/layout";
import { useListNav } from "../../lib/listNav";
import { signalReady } from "../../lib/perf";

const ROW_HEIGHT = 56;
const PAGE = 100;

/** Repositories view (all known repos) and Starred view (`starredOnly`). */
export function ReposView({
  starredOnly,
  onNavigate,
}: {
  starredOnly: boolean;
  onNavigate: (v: ViewRef) => void;
}) {
  const [query, setQuery] = useState("");
  const q = useInfiniteQuery({
    queryKey: ["repos", "list", starredOnly, query],
    queryFn: ({ pageParam }) =>
      commands.reposList({ query, starred_only: starredOnly }, { offset: pageParam, limit: PAGE }),
    initialPageParam: 0,
    getNextPageParam: (last) => last.next ?? undefined,
    placeholderData: (prev) => prev,
  });
  const setPinned = useMutation({
    mutationFn: (v: { id: number; pinned: boolean }) => commands.repoSetPinned(v.id, v.pinned),
  });

  const rows = q.data?.pages.flatMap((p) => p.items) ?? [];
  const scroller = useRef<HTMLDivElement>(null);
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual API
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  });
  const nav = useListNav({
    count: rows.length,
    onOpen: (i) => onNavigate({ kind: "repo", repoId: rows[i]!.id }),
    onSelect: (i) => virtualizer.scrollToIndex(i),
  });

  const items = virtualizer.getVirtualItems();
  const lastIndex = items.at(-1)?.index ?? -1;
  useEffect(() => {
    if (lastIndex >= rows.length - 10 && q.hasNextPage && !q.isFetchingNextPage) void q.fetchNextPage();
  }, [lastIndex, rows.length, q]);

  const readyKey = starredOnly ? "starred" : "repos";
  useEffect(() => {
    if (q.isSuccess) signalReady(readyKey);
  }, [q.isSuccess, readyKey]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-3 border-b border-border p-3">
        <h1 className="text-lg font-semibold">{starredOnly ? "Starred" : "Repositories"}</h1>
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Filter by name"
          aria-label="Filter repositories by name"
          className="ml-auto w-64 rounded-md border border-border bg-bg px-2 py-1"
        />
      </div>
      {q.isError ? <p role="alert" className="p-4">Could not read the local repository cache.</p> : null}
      {q.isSuccess && rows.length === 0 ? (
        <p className="p-4 text-muted">
          {query ? "No repositories match." : starredOnly ? "No starred repositories yet." : "No repositories cached yet."}
        </p>
      ) : null}
      <div
        ref={scroller}
        data-list-nav
        tabIndex={0}
        onKeyDown={nav.onKeyDown}
        className="min-h-0 flex-1 overflow-auto outline-none"
      >
        <ul role="list" className="relative" style={{ height: virtualizer.getTotalSize() }}>
          {items.map((v) => {
            const r = rows[v.index]!;
            return (
              <li
                key={r.id}
                data-index={v.index}
                className={`absolute inset-x-0 flex items-center gap-3 border-b border-border px-4 ${
                  v.index === nav.selected ? "bg-raised" : "hover:bg-raised"
                }`}
                style={{ top: v.start, height: ROW_HEIGHT }}
                aria-current={v.index === nav.selected}
              >
                <button
                  className="flex min-w-0 flex-1 flex-col items-start text-left"
                  onClick={() => {
                    nav.setSelected(v.index);
                    onNavigate({ kind: "repo", repoId: r.id });
                  }}
                >
                  <span className="max-w-full truncate">
                    <span className="text-muted">{r.owner} / </span>
                    <span className="font-medium">{r.name}</span>
                    {r.starred ? <span className="ml-2 text-muted" title="Starred">★</span> : null}
                  </span>
                  <span className="max-w-full truncate text-muted">{r.description ?? "No description."}</span>
                </button>
                <span className="w-24 shrink-0 text-muted tabular-nums">
                  {r.open_prs} PRs · {r.open_issues} issues
                </span>
                <span className="w-20 shrink-0 text-muted">{r.language ?? ""}</span>
                <span className="w-16 shrink-0 text-right text-muted tabular-nums">★ {r.stars.toLocaleString()}</span>
                <button
                  className="w-16 shrink-0 rounded-md border border-border px-2 py-1 hover:bg-bg"
                  aria-pressed={r.pinned}
                  onClick={() => setPinned.mutate({ id: r.id, pinned: !r.pinned })}
                >
                  {r.pinned ? "Unpin" : "Pin"}
                </button>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
