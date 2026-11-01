import { useQuery } from "@tanstack/react-query";
import { commands } from "../bindings";
import type { ViewRef } from "../lib/layout";

const NAV: Array<{ label: string; view: ViewRef }> = [
  { label: "Home", view: { kind: "home" } },
  { label: "Inbox", view: { kind: "inbox" } },
  { label: "Pull requests", view: { kind: "pulls" } },
  { label: "Issues", view: { kind: "issues" } },
  { label: "Review requests", view: { kind: "reviews" } },
  { label: "Starred", view: { kind: "starred" } },
  { label: "Repositories", view: { kind: "repos" } },
];

export function Sidebar({ onNavigate }: { onNavigate: (v: ViewRef) => void }) {
  const pinned = useQuery({ queryKey: ["repos", "pinned"], queryFn: commands.reposPinned });
  return (
    <nav className="row-span-1 flex flex-col gap-px overflow-y-auto border-r border-border bg-panel p-2">
      <div className="px-2 pt-1 pb-3 text-sm font-semibold">Forkline</div>
      {NAV.map((n) => (
        <button
          key={n.label}
          className="rounded-md px-2 py-1.5 text-left hover:bg-raised"
          onClick={() => onNavigate(n.view)}
        >
          {n.label}
        </button>
      ))}
      <div className="mt-4 px-2 pb-1 text-xs font-semibold text-muted uppercase">Pinned</div>
      {pinned.data?.map((r) => (
        <button
          key={r.id}
          className="flex items-center gap-2 rounded-md px-2 py-1.5 text-left hover:bg-raised"
          onClick={() => onNavigate({ kind: "repo", repoId: r.id })}
          title={`${r.open_prs} open PRs, ${r.open_issues} open issues`}
        >
          <span className="truncate">{r.name}</span>
          <span className="ml-auto text-xs text-muted tabular-nums">{r.open_prs}</span>
        </button>
      ))}
    </nav>
  );
}
