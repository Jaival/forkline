import { useEffect, useState } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { commands, type InboxFilter, type InboxRow } from "../../bindings";
import type { ViewRef } from "../../lib/layout";
import { signalReady } from "../../lib/perf";
import { VirtualList } from "../../lib/VirtualList";
import { useAction } from "../issue/useAction";
import { timeAgo } from "../lists/format";
import { viewFor } from "../lists/IssueRowItem";
import { type InboxItem, withGroupHeaders } from "./groups";

const PAGE = 50;
const ROW_HEIGHT = 40;

export function InboxView({ onNavigate }: { onNavigate: (v: ViewRef) => void }) {
  const [grouped, setGrouped] = useState(false);
  const [unreadOnly, setUnreadOnly] = useState(false);
  const action = useAction();

  const filter: InboxFilter = { repo_id: null, unread_only: unreadOnly, grouped };
  const list = useInfiniteQuery({
    queryKey: ["inbox", "list", filter],
    queryFn: ({ pageParam }) => commands.inboxList(filter, { offset: pageParam, limit: PAGE }),
    initialPageParam: 0,
    getNextPageParam: (last) => last.next ?? undefined,
  });
  const rows = list.data?.pages.flatMap((p) => p.items) ?? [];
  const items: InboxItem[] = grouped ? withGroupHeaders(rows) : rows.map((row) => ({ type: "row", row }));

  useEffect(() => {
    if (list.isSuccess) signalReady("inbox");
  }, [list.isSuccess]);

  const open = (row: InboxRow) => {
    if (row.issue_id !== null) onNavigate(viewFor({ id: row.issue_id, is_pr: row.is_pr }));
  };

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-4 px-6 pt-6 pb-3">
        <h1 className="text-xl font-semibold">Inbox</h1>
        <label className="flex items-center gap-1">
          <input type="checkbox" checked={unreadOnly} onChange={(e) => setUnreadOnly(e.target.checked)} /> Unread only
        </label>
        <label className="flex items-center gap-1">
          <input type="checkbox" checked={grouped} onChange={(e) => setGrouped(e.target.checked)} /> Group by repository
        </label>
        {action.error ? <p role="alert" className="text-closed">{action.error}</p> : null}
      </header>
      {list.isError ? <p className="p-6" role="alert">Could not read the local cache.</p> : null}
      {list.isSuccess && rows.length === 0 ? <p className="p-6 text-muted">You are all caught up.</p> : null}
      <div className="min-h-0 flex-1 border-t border-border">
        <VirtualList
          label="Notifications"
          items={items}
          rowHeight={ROW_HEIGHT}
          getKey={(i) => (i.type === "header" ? `h:${i.repo}` : i.row.id)}
          canSelect={(i) => i.type !== "header"}
          onOpen={(i) => {
            if (i.type !== "header") open(i.row);
          }}
          onEndReached={() => {
            if (list.hasNextPage && !list.isFetchingNextPage) void list.fetchNextPage();
          }}
          renderRow={(i) =>
            i.type === "header" ? (
              <div className="flex h-full items-end bg-panel px-4 pb-1 font-semibold">{i.repo}</div>
            ) : (
              <div className="flex h-full items-center gap-3 border-b border-border px-4 hover:bg-raised">
                <span
                  aria-label={i.row.unread ? "Unread" : "Read"}
                  className={`size-2 shrink-0 rounded-full ${i.row.unread ? "bg-accent" : "bg-transparent"}`}
                />
                <button
                  className="flex min-w-0 flex-1 items-center gap-3 text-left disabled:cursor-default"
                  disabled={i.row.issue_id === null}
                  onClick={() => open(i.row)}
                >
                  {grouped ? null : <span className="w-52 shrink-0 truncate text-muted">{i.row.repo}</span>}
                  <span className={`min-w-0 flex-1 truncate ${i.row.unread ? "font-semibold" : ""}`}>{i.row.title}</span>
                  <span className="w-32 shrink-0 truncate text-muted">{i.row.reason.replaceAll("_", " ")}</span>
                  <span className="w-10 shrink-0 text-right text-muted">{timeAgo(i.row.updated_at)}</span>
                </button>
                {i.row.pending ? <span className="rounded bg-raised px-1.5 text-xs text-muted">sending…</span> : null}
                {i.row.unread ? (
                  <button
                    className="rounded px-2 hover:bg-bg"
                    onClick={() => void action.run({ kind: "notification_read", id: i.row.id })}
                  >
                    Mark read
                  </button>
                ) : null}
                <button
                  className="rounded px-2 hover:bg-bg"
                  onClick={() => void action.run({ kind: "notification_done", id: i.row.id })}
                >
                  Done
                </button>
              </div>
            )
          }
        />
      </div>
    </div>
  );
}
