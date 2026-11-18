import { type FormEvent, useEffect, useState } from "react";
import { useInfiniteQuery } from "@tanstack/react-query";
import { commands, type IssueFilter, type IssueState } from "../../bindings";
import { type ViewRef, viewTitle } from "../../lib/layout";
import { signalReady } from "../../lib/perf";
import { VirtualList } from "../../lib/VirtualList";
import { IssueRowItem, ROW_HEIGHT, viewFor } from "./IssueRowItem";

type Kind = "pulls" | "issues" | "reviews";
type Text = { author: string; assignee: string; label: string };

const PAGE = 50;
const input = "h-7 w-32 rounded border border-border bg-bg px-2";

/** Pull requests, Issues and Review requests. `repoId` limits the list to one repo. */
export function ListView({ view, onNavigate }: { view: ViewRef & { kind: Kind }; onNavigate: (v: ViewRef) => void }) {
  const repoId = view.kind === "reviews" ? undefined : view.repoId;
  const [state, setState] = useState<IssueState | "all">("open");
  const [draft, setDraft] = useState<Text>({ author: "", assignee: "", label: "" });
  const [text, setText] = useState<Text>(draft);

  const filter: IssueFilter = {
    repo_id: repoId ?? null,
    state: state === "all" ? null : state,
    author: text.author || null,
    assignee: text.assignee || null,
    label: text.label || null,
    is_pr: view.kind === "issues" ? false : view.kind === "pulls" ? true : null,
    review_requested: view.kind === "reviews",
  };
  const list = useInfiniteQuery({
    queryKey: ["issues", "list", filter],
    queryFn: ({ pageParam }) => commands.issueList(filter, { offset: pageParam, limit: PAGE }),
    initialPageParam: 0,
    getNextPageParam: (last) => last.next ?? undefined,
  });
  const items = list.data?.pages.flatMap((p) => p.items) ?? [];

  useEffect(() => {
    if (list.isSuccess) signalReady(`list:${view.kind}`);
  }, [list.isSuccess, view.kind]);

  const apply = (e: FormEvent) => {
    e.preventDefault();
    setText({ author: draft.author.trim(), assignee: draft.assignee.trim(), label: draft.label.trim() });
  };
  const field = (key: keyof Text, label: string) => (
    <input
      aria-label={label}
      placeholder={label}
      className={input}
      value={draft[key]}
      onChange={(e) => setDraft({ ...draft, [key]: e.target.value })}
    />
  );

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-3 px-6 pt-6 pb-3">
        <h1 className="text-xl font-semibold">{viewTitle(view)}</h1>
        {repoId !== undefined ? <span className="text-muted">repo {repoId}</span> : null}
      </header>
      <form onSubmit={apply} className="flex flex-wrap items-center gap-2 border-b border-border px-6 pb-3">
        <select
          aria-label="State"
          className={input}
          value={state}
          onChange={(e) => setState(e.target.value as IssueState | "all")}
        >
          <option value="open">Open</option>
          <option value="closed">Closed</option>
          {view.kind !== "issues" ? <option value="merged">Merged</option> : null}
          <option value="all">All</option>
        </select>
        {field("author", "Author")}
        {view.kind !== "reviews" ? field("assignee", "Assignee") : null}
        {field("label", "Label")}
        <button className="h-7 rounded border border-border px-3 hover:bg-raised" type="submit">Filter</button>
      </form>
      {list.isError ? <p className="p-6" role="alert">Could not read the local cache.</p> : null}
      {list.isSuccess && items.length === 0 ? <p className="p-6 text-muted">Nothing matches.</p> : null}
      <div className="min-h-0 flex-1">
        <VirtualList
          label={viewTitle(view)}
          items={items}
          rowHeight={ROW_HEIGHT}
          getKey={(item) => item.id}
          renderRow={(item) => <IssueRowItem item={item} onNavigate={onNavigate} />}
          onOpen={(item) => onNavigate(viewFor(item))}
          onEndReached={() => {
            if (list.hasNextPage && !list.isFetchingNextPage) void list.fetchNextPage();
          }}
        />
      </div>
    </div>
  );
}
