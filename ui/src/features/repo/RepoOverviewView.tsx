import { useEffect } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { commands, type RepoOverview } from "../../bindings";
import type { ViewRef } from "../../lib/layout";
import { signalReady } from "../../lib/perf";
import { languageColor, languageShares, sparklinePoints } from "./sparkline";

const CHECK_STYLE: Record<string, string> = {
  SUCCESS: "bg-open",
  FAILURE: "bg-closed",
  ERROR: "bg-closed",
  PENDING: "bg-muted",
};

const date = (secs: number) => new Date(secs * 1000).toLocaleDateString();

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-md border border-border p-4">
      <h2 className="mb-2 text-xs font-semibold text-muted uppercase">{title}</h2>
      {children}
    </section>
  );
}

function Empty({ children }: { children: string }) {
  return <p className="text-muted">{children}</p>;
}

function Overview({ o, onNavigate }: { o: RepoOverview; onNavigate: (v: ViewRef) => void }) {
  const setPinned = useMutation({ mutationFn: () => commands.repoSetPinned(o.id, !o.pinned) });
  const shares = languageShares(o.languages);
  const history = o.star_history.map((p) => p.stars);

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-4 p-6">
      <header>
        <div className="flex items-start gap-3">
          <h1 className="text-xl font-semibold">
            <span className="text-muted">{o.owner} / </span>
            {o.name}
          </h1>
          <button
            className="ml-auto rounded-md border border-border px-3 py-1 hover:bg-raised"
            aria-pressed={o.pinned}
            disabled={setPinned.isPending}
            onClick={() => setPinned.mutate()}
          >
            {o.pinned ? "Unpin" : "Pin"}
          </button>
        </div>
        <p className="mt-1 text-muted">{o.description ?? "No description."}</p>
        <p className="mt-2 flex flex-wrap gap-x-4 text-muted tabular-nums">
          <span>{o.starred ? "★" : "☆"} {o.stars.toLocaleString()} stars</span>
          <span>{o.forks.toLocaleString()} forks</span>
          <span>Default branch: {o.default_branch}</span>
          <span>License: {o.license ?? "none"}</span>
        </p>
      </header>

      <div className="grid grid-cols-2 gap-4">
        <button
          className="rounded-md border border-border p-4 text-left hover:bg-raised"
          onClick={() => onNavigate({ kind: "pulls", repoId: o.id })}
        >
          <div className="text-2xl font-semibold tabular-nums">{o.open_prs}</div>
          <div className="text-muted">Open pull requests</div>
        </button>
        <button
          className="rounded-md border border-border p-4 text-left hover:bg-raised"
          onClick={() => onNavigate({ kind: "issues", repoId: o.id })}
        >
          <div className="text-2xl font-semibold tabular-nums">{o.open_issues}</div>
          <div className="text-muted">Open issues</div>
        </button>
      </div>

      <Section title="Languages">
        {shares.length === 0 ? (
          <Empty>No language data.</Empty>
        ) : (
          <>
            <div className="flex h-2 overflow-hidden rounded-full" role="img" aria-label="Language breakdown">
              {shares.map((s) => (
                <div key={s.name} style={{ width: `${s.percent}%`, background: languageColor(s.name) }} />
              ))}
            </div>
            <ul className="mt-2 flex flex-wrap gap-x-4">
              {shares.map((s) => (
                <li key={s.name} className="flex items-center gap-1.5">
                  <span className="size-2 rounded-full" style={{ background: languageColor(s.name) }} />
                  {s.name} <span className="text-muted tabular-nums">{s.percent.toFixed(1)}%</span>
                </li>
              ))}
            </ul>
          </>
        )}
      </Section>

      <Section title="Star history">
        {history.length < 2 ? (
          <Empty>Not enough history yet.</Empty>
        ) : (
          <svg viewBox="0 0 300 40" className="h-10 w-full" preserveAspectRatio="none" role="img" aria-label="Star history">
            <polyline
              points={sparklinePoints(history, 300, 40)}
              fill="none"
              stroke="var(--fl-accent)"
              strokeWidth="2"
              vectorEffect="non-scaling-stroke"
            />
          </svg>
        )}
      </Section>

      <Section title={`Recent commits on ${o.default_branch}`}>
        {o.commits.length === 0 ? (
          <Empty>No cached commits. Pin this repository to sync them.</Empty>
        ) : (
          <ul className="divide-y divide-border">
            {o.commits.map((c) => (
              <li key={c.oid} className="flex items-center gap-3 py-1.5">
                <span
                  className={`size-2 shrink-0 rounded-full ${CHECK_STYLE[c.check_state ?? ""] ?? "border border-border"}`}
                  title={c.check_state ?? "No checks"}
                />
                <span className="truncate">{c.headline}</span>
                <span className="ml-auto shrink-0 text-muted">
                  {c.author ?? "unknown"} · <code className="font-mono">{c.oid.slice(0, 7)}</code> · {date(c.authored_at)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </Section>

      <div className="grid grid-cols-2 gap-4">
        <Section title="Latest releases">
          {o.releases.length === 0 ? (
            <Empty>No releases.</Empty>
          ) : (
            <ul>
              {o.releases.map((r) => (
                <li key={r.tag} className="flex gap-2 py-0.5">
                  <span className="font-mono">{r.tag}</span>
                  <span className="truncate text-muted">{r.name}</span>
                  <span className="ml-auto text-muted">{r.published_at ? date(r.published_at) : ""}</span>
                </li>
              ))}
            </ul>
          )}
        </Section>
        <Section title="Top contributors">
          {o.contributors.length === 0 ? (
            <Empty>No contributor data.</Empty>
          ) : (
            <ul>
              {o.contributors.map((c) => (
                <li key={c.login} className="flex py-0.5">
                  {c.login}
                  <span className="ml-auto text-muted tabular-nums">{c.contributions}</span>
                </li>
              ))}
            </ul>
          )}
        </Section>
      </div>
    </div>
  );
}

export function RepoOverviewView({ repoId, onNavigate }: { repoId: number; onNavigate: (v: ViewRef) => void }) {
  const q = useQuery({ queryKey: ["repos", "overview", repoId], queryFn: () => commands.repoOverview(repoId) });

  useEffect(() => {
    if (q.isSuccess) signalReady(`repo:${repoId}`);
  }, [q.isSuccess, repoId]);

  if (q.isPending) return <p className="p-6 text-muted">Loading repository…</p>;
  if (q.isError) return <p role="alert" className="p-6">Repository not found in the local cache.</p>;
  return (
    <div className="h-full overflow-auto">
      <Overview o={q.data} onNavigate={onNavigate} />
    </div>
  );
}
