import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { commands } from "../../bindings";
import type { ViewRef } from "../../lib/layout";
import { signalReady } from "../../lib/perf";
import { VirtualList } from "../../lib/VirtualList";
import { IssueRowItem, ROW_HEIGHT, viewFor } from "../lists/IssueRowItem";

export function HomeView({ onNavigate }: { onNavigate: (view: ViewRef) => void }) {
  const feed = useQuery({
    queryKey: ["issues", "home", 0],
    queryFn: () => commands.homeFeed({ offset: 0, limit: 50 }),
  });

  // Rows are committed to the DOM once this effect runs.
  useEffect(() => {
    if (feed.isSuccess) signalReady("home");
  }, [feed.isSuccess]);

  return (
    <div className="flex h-full flex-col">
      <h1 className="px-6 pt-6 pb-4 text-xl font-semibold">Home</h1>
      {feed.isPending ? <p className="px-6 text-muted">Loading cached activity…</p> : null}
      {feed.isError ? <p className="px-6" role="alert">Could not read the local activity cache.</p> : null}
      {feed.data?.items.length === 0 ? <p className="px-6 text-muted">No cached activity yet.</p> : null}
      <div className="min-h-0 flex-1">
        <VirtualList
          label="Recent activity"
          items={feed.data?.items ?? []}
          rowHeight={ROW_HEIGHT}
          getKey={(item) => item.id}
          renderRow={(item) => <IssueRowItem item={item} onNavigate={onNavigate} />}
          onOpen={(item) => onNavigate(viewFor(item))}
        />
      </div>
    </div>
  );
}
