import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { commands } from "../../bindings";
import { type ViewRef, viewTitle } from "../../lib/layout";
import { driver } from "../../shell/driver";
import { filterItems, onOpenPalette, type PaletteItem } from "./paletteState";

const VIEWS: PaletteItem[] = (
  [
    ["Home", { kind: "home" }],
    ["Inbox", { kind: "inbox" }],
    ["Pull requests", { kind: "pulls" }],
    ["Issues", { kind: "issues" }],
    ["Review requests", { kind: "reviews" }],
    ["Starred", { kind: "starred" }],
    ["Repositories", { kind: "repos" }],
  ] as Array<[string, ViewRef]>
).map(([label, view]) => ({ id: `view:${label}`, label, hint: "Go to", view }));

const MAX_ITEMS = 30;

/** Ctrl+K palette: views, pinned repos, repo name search, open tabs. Enter opens here, Shift+Enter in a new tab. */
export function Palette() {
  const [open, setOpen] = useState(false);
  useEffect(() => onOpenPalette(() => setOpen(true)), []);
  return open ? <PaletteDialog onClose={() => setOpen(false)} /> : null;
}

function PaletteDialog({ onClose }: { onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => input.current?.focus(), []);

  const pinned = useQuery({ queryKey: ["repos", "pinned"], queryFn: commands.reposPinned });
  const q = query.trim();
  const found = useQuery({
    queryKey: ["repos", "palette", q],
    queryFn: () => commands.reposList({ query: q, starred_only: false }, { offset: 0, limit: 10 }),
    enabled: q.length > 0,
  });

  const tabs: PaletteItem[] = (driver.layout?.panes ?? []).flatMap((p) =>
    p.tabs.map((t) => ({ id: `tab:${t.id}`, label: viewTitle(t.view), hint: "Open tab", view: t.view })),
  );
  const repoItems = new Map<number, PaletteItem>();
  for (const r of pinned.data ?? []) {
    repoItems.set(r.id, { id: `repo:${r.id}`, label: `${r.owner}/${r.name}`, hint: "Pinned repository", view: { kind: "repo", repoId: r.id } });
  }
  for (const r of found.data?.items ?? []) {
    repoItems.set(r.id, { id: `repo:${r.id}`, label: `${r.owner}/${r.name}`, hint: "Repository", view: { kind: "repo", repoId: r.id } });
  }
  const items = [
    ...filterItems(VIEWS, query),
    ...filterItems([...repoItems.values()], query),
    ...filterItems(tabs, query),
    ...(q ? [{ id: "search", label: `Search for “${q}”`, hint: "Search", view: { kind: "search", q } as ViewRef }] : []),
  ].slice(0, MAX_ITEMS);
  const current = Math.min(selected, items.length - 1);

  const choose = (item: PaletteItem | undefined, newTab: boolean) => {
    if (!item) return;
    onClose();
    if (newTab) driver.dispatch?.({ type: "newTab", view: item.view });
    else driver.dispatch?.({ type: "navigate", view: item.view });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
    else if (e.key === "ArrowDown") setSelected(Math.min(current + 1, items.length - 1));
    else if (e.key === "ArrowUp") setSelected(Math.max(current - 1, 0));
    else if (e.key === "Enter") choose(items[current], e.shiftKey);
    else return;
    e.preventDefault();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/30 pt-24" onMouseDown={onClose}>
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        className="w-[560px] overflow-hidden rounded-lg border border-border bg-panel shadow-xl"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={onKeyDown}
      >
        <input
          ref={input}
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-list"
          aria-activedescendant={items[current] ? `palette-${items[current].id}` : undefined}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setSelected(0);
          }}
          placeholder="Go to a view, repository or tab…"
          className="w-full border-b border-border bg-transparent px-4 py-3 outline-none"
        />
        <ul id="palette-list" role="listbox" className="max-h-80 overflow-auto py-1">
          {items.length === 0 ? <li className="px-4 py-2 text-muted">No matches.</li> : null}
          {items.map((item, i) => (
            <li
              key={item.id}
              id={`palette-${item.id}`}
              role="option"
              aria-selected={i === current}
              className={`flex cursor-default items-center gap-3 px-4 py-1.5 ${i === current ? "bg-raised" : ""}`}
              onMouseMove={() => setSelected(i)}
              onClick={(e) => choose(item, e.shiftKey)}
            >
              <span className="truncate">{item.label}</span>
              <span className="ml-auto shrink-0 text-xs text-muted">{item.hint}</span>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
