import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import {
  commands,
  type DiffFile,
  type DiffSide,
  type DiffSummary,
  type DraftReviewComment,
  type ReviewThread,
} from "../../bindings";
import { signalReady } from "../../lib/perf";
import { errorText } from "../pr/shared";
import { DiffLines } from "./DiffLines";
import { buildRows, flattenTree, type Composer } from "./rows";
import { ThreadCard } from "./ThreadCard";

type Source = {
  id: string;
  summary: () => Promise<DiffSummary>;
  file: (path: string) => Promise<DiffFile>;
};

export function DiffViewer({
  prId,
  headOid,
  threads,
  drafts,
  onAddDraft,
  onRemoveDraft,
}: {
  prId: number;
  headOid: string;
  threads: ReviewThread[];
  drafts: DraftReviewComment[];
  onAddDraft: (d: DraftReviewComment) => void;
  onRemoveDraft: (d: DraftReviewComment) => void;
}) {
  const qc = useQueryClient();
  const [fixture, setFixture] = useState<Source | null>(null);
  const source: Source = fixture ?? {
    id: `pr:${prId}:${headOid}`,
    summary: () => commands.diffSummary(prId),
    file: (path) => commands.diffFile(prId, path),
  };

  const summary = useQuery({
    queryKey: ["diff", "summary", source.id],
    queryFn: source.summary,
  });
  const viewed = useQuery({
    queryKey: ["pr", "viewed", prId, headOid],
    queryFn: () => commands.prViewedPaths(prId),
  });
  const setViewed = useMutation({
    mutationFn: (v: { path: string; viewed: boolean }) => commands.prSetViewed(prId, v.path, v.viewed),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["pr", "viewed", prId] }),
  });

  const [selected, setSelected] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const [filter, setFilter] = useState("");
  const [composer, setComposer] = useState<Composer>(null);

  const files = summary.data?.files;
  const current = selected ?? files?.[0]?.path ?? null;

  const file = useQuery({
    queryKey: ["diff", "file", source.id, current],
    queryFn: () => source.file(current as string),
    enabled: current !== null,
  });

  const built = useMemo(
    () => (file.data ? buildRows(file.data, threads, drafts, composer) : null),
    [file.data, threads, drafts, composer],
  );

  useEffect(() => {
    if (built) signalReady(`diff:${prId}`);
  }, [built, prId]);

  if (summary.isPending) return <p className="p-6 text-muted">Loading changed files…</p>;
  if (summary.isError || !files) {
    return (
      <div className="p-6">
        <p role="alert" className="font-semibold">Diff not available</p>
        <p className="mt-1 text-muted">
          The diff is read from a local git clone, which is not ready yet: {errorText(summary.error)}
        </p>
        {import.meta.env.DEV ? (
          <button
            className="mt-3 rounded border border-border px-3 py-1 hover:bg-raised"
            onClick={() =>
              void import("./fixture").then((f) =>
                setFixture({
                  id: "fixture",
                  summary: () => Promise.resolve(f.fixtureSummary()),
                  file: (p) => Promise.resolve(f.fixtureFile(p)),
                }),
              )
            }
          >
            Load large fixture diff (dev only)
          </button>
        ) : null}
      </div>
    );
  }
  if (files.length === 0) return <p className="p-6 text-muted">No changed files.</p>;

  const viewedSet = new Set(viewed.data ?? []);
  const shown = filter ? files.filter((f) => f.path.toLowerCase().includes(filter.toLowerCase())) : files;
  const meta = files.find((f) => f.path === current);

  const addDraft = (side: DiffSide, line: number, body: string) => {
    if (current === null) return;
    onAddDraft({ path: current, side, line, start_line: null, body_md: body });
    setComposer(null);
  };

  return (
    <div className="flex h-full min-h-0">
      <aside className="flex w-72 shrink-0 flex-col border-r border-border">
        <input
          className="m-2 rounded border border-border bg-bg px-2 py-1"
          placeholder={`Filter ${files.length} files`}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        {fixture ? <p className="px-3 pb-1 text-xs text-muted">Showing dev fixture</p> : null}
        <FileTree
          paths={shown.map((f) => f.path)}
          flat={filter !== ""}
          collapsed={collapsed}
          onToggle={(d) =>
            setCollapsed((s) => {
              const n = new Set(s);
              if (!n.delete(d)) n.add(d);
              return n;
            })
          }
          selected={current}
          onSelect={(p) => {
            setSelected(p);
            setComposer(null);
          }}
          viewed={viewedSet}
          stats={new Map(files.map((f) => [f.path, f]))}
        />
      </aside>
      <section className="flex min-w-0 flex-1 flex-col">
        {meta ? (
          <header className="flex items-center gap-3 border-b border-border px-3 py-1.5">
            <span className="min-w-0 flex-1 truncate font-mono">{meta.old_path ? `${meta.old_path} → ` : ""}{meta.path}</span>
            <span className="text-open">+{meta.additions}</span>
            <span className="text-closed">−{meta.deletions}</span>
            <label className="flex items-center gap-1">
              <input
                type="checkbox"
                checked={viewedSet.has(meta.path)}
                onChange={(e) => setViewed.mutate({ path: meta.path, viewed: e.target.checked })}
              />
              Viewed
            </label>
          </header>
        ) : null}
        {setViewed.isError ? <p role="alert" className="px-3 text-xs text-closed">Could not save: {errorText(setViewed.error)}</p> : null}
        <div className="min-h-0 flex-1">
          {file.isPending ? <p className="p-6 text-muted">Loading file…</p> : null}
          {file.isError ? <p role="alert" className="p-6">Could not load this file: {errorText(file.error)}</p> : null}
          {meta?.binary ? <p className="p-6 text-muted">Binary file not shown.</p> : null}
          {built && !meta?.binary ? (
            <div className="flex h-full flex-col">
              {built.unplaced.length > 0 ? (
                <div className="max-h-48 overflow-auto border-b border-border py-1">
                  <p className="px-3 text-xs text-muted">Comments that no longer match a line</p>
                  {built.unplaced.map((t) => <ThreadCard key={t.id} thread={t} />)}
                </div>
              ) : null}
              <div className="min-h-0 flex-1">
                <DiffLines
                  key={source.id + current}
                  rows={built.rows}
                  onOpenComposer={(side, line) => setComposer({ side, line })}
                  onCloseComposer={() => setComposer(null)}
                  onSaveDraft={addDraft}
                  onRemoveDraft={onRemoveDraft}
                />
              </div>
            </div>
          ) : null}
        </div>
      </section>
    </div>
  );
}

function FileTree({
  paths,
  flat,
  collapsed,
  onToggle,
  selected,
  onSelect,
  viewed,
  stats,
}: {
  paths: string[];
  flat: boolean;
  collapsed: ReadonlySet<string>;
  onToggle: (dir: string) => void;
  selected: string | null;
  onSelect: (path: string) => void;
  viewed: ReadonlySet<string>;
  stats: Map<string, { additions: number; deletions: number }>;
}) {
  const entries = useMemo(
    () =>
      flat
        ? paths.map((p) => ({ kind: "file" as const, path: p, name: p, depth: 0 }))
        : flattenTree(paths, collapsed),
    [paths, flat, collapsed],
  );
  const ref = useRef<HTMLDivElement>(null);
  // eslint-disable-next-line react-hooks/incompatible-library
  const virt = useVirtualizer({
    count: entries.length,
    getScrollElement: () => ref.current,
    estimateSize: () => 24,
    overscan: 10,
  });
  return (
    <div ref={ref} className="min-h-0 flex-1 overflow-auto">
      <div className="relative" style={{ height: virt.getTotalSize() }}>
        {virt.getVirtualItems().map((v) => {
          const e = entries[v.index];
          if (!e) return null;
          const s = stats.get(e.path);
          return (
            <button
              key={e.path}
              className={`absolute left-0 top-0 flex h-6 w-full items-center gap-1 pr-2 text-left hover:bg-raised ${
                e.path === selected ? "bg-raised font-semibold" : ""
              }`}
              style={{ transform: `translateY(${v.start}px)`, paddingLeft: 8 + e.depth * 12 }}
              onClick={() => (e.kind === "dir" ? onToggle(e.path) : onSelect(e.path))}
            >
              <span className="w-3 text-muted">{e.kind === "dir" ? (collapsed.has(e.path) ? "▸" : "▾") : viewed.has(e.path) ? "✓" : ""}</span>
              <span className={`min-w-0 flex-1 truncate ${viewed.has(e.path) ? "text-muted" : ""}`}>{e.name}</span>
              {s ? <span className="text-xs text-muted">+{s.additions} −{s.deletions}</span> : null}
            </button>
          );
        })}
      </div>
    </div>
  );
}
