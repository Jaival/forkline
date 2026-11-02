import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { commands, type DraftReviewComment, type PrDetail } from "../../bindings";
import { signalReady } from "../../lib/perf";
import { DiffViewer } from "../diff/DiffViewer";
import { MergeBox, ReviewForm } from "./PrActions";
import { Body, when } from "./shared";

type Tab = "conversation" | "files" | "commits" | "checks";

const TONE: Record<string, string> = {
  SUCCESS: "text-open",
  APPROVED: "text-open",
  MERGEABLE: "text-open",
  FAILURE: "text-closed",
  ERROR: "text-closed",
  CHANGES_REQUESTED: "text-closed",
  CONFLICTING: "text-closed",
};

const label = (s: string) => s.toLowerCase().replaceAll("_", " ");

function Chip({ name, value }: { name: string; value: string | null }) {
  return (
    <span className="rounded border border-border px-2 py-0.5 text-xs">
      <span className="text-muted">{name}: </span>
      <span className={TONE[value ?? ""] ?? ""}>{value ? label(value) : "unknown"}</span>
    </span>
  );
}

export function PrView({ id }: { id: number }) {
  const pr = useQuery({ queryKey: ["pr", "detail", id], queryFn: () => commands.prGet(id) });
  const threads = useQuery({
    queryKey: ["pr", "threads", id],
    queryFn: async () => {
      const all = [];
      for (let offset: number | null = 0; offset !== null; ) {
        const page = await commands.prThreads(id, { offset, limit: 100 });
        all.push(...page.items);
        offset = page.next;
      }
      return all;
    },
  });
  const [tab, setTab] = useState<Tab>("conversation");
  const [drafts, setDrafts] = useState<DraftReviewComment[]>([]);

  useEffect(() => {
    if (pr.isSuccess) signalReady(`pr:${id}`);
  }, [pr.isSuccess, id]);

  if (pr.isPending) return <div className="p-6 text-muted">Loading cached pull request…</div>;
  if (pr.isError) return <div className="p-6" role="alert">Could not load this pull request from the local cache.</div>;
  const d = pr.data;

  const tabs: Array<[Tab, string]> = [
    ["conversation", `Conversation ${d.row.comments}`],
    ["commits", `Commits ${d.commits.length}`],
    ["checks", `Checks ${d.checks.length}`],
    ["files", `Files ${d.changed_files}${drafts.length ? ` · ${drafts.length} pending` : ""}`],
  ];

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-border px-6 pt-4">
        <p className="text-sm text-muted">{d.row.repo} #{d.row.number}</p>
        <h1 className="mb-2 text-xl font-semibold">{d.row.title}</h1>
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <span className={`rounded px-2 py-0.5 text-xs font-semibold text-white ${d.row.state === "open" ? "bg-open" : d.row.state === "merged" ? "bg-merged" : "bg-closed"}`}>
            {d.is_draft ? "Draft" : label(d.row.state)}
          </span>
          {d.pending_fields.length > 0 ? <span className="text-xs text-muted">Waiting to sync…</span> : null}
          <span className="font-mono text-xs">{d.head_ref} → {d.base_ref}</span>
          <Chip name="Merge" value={d.mergeable} />
          <Chip name="Review" value={d.review_decision} />
          <Chip name="Checks" value={d.check_state} />
          <span className="text-xs"><span className="text-open">+{d.additions}</span> <span className="text-closed">−{d.deletions}</span> in {d.changed_files} files</span>
        </div>
        <nav className="flex gap-1">
          {tabs.map(([t, text]) => (
            <button
              key={t}
              className={`rounded-t px-3 py-1.5 ${tab === t ? "border-b-2 border-accent font-semibold" : "text-muted hover:bg-raised"}`}
              onClick={() => setTab(t)}
            >
              {text}
            </button>
          ))}
        </nav>
      </header>
      <div className="min-h-0 flex-1">
        {tab === "files" ? (
          <DiffViewer
            prId={id}
            headOid={d.head_oid}
            threads={threads.data ?? []}
            drafts={drafts}
            onAddDraft={(x) => setDrafts((l) => [...l, x])}
            onRemoveDraft={(x) => setDrafts((l) => l.filter((y) => y !== x))}
          />
        ) : (
          <div className="h-full overflow-auto p-6">
            {tab === "conversation" ? (
              <Conversation d={d} drafts={drafts} onSubmitted={() => setDrafts([])} />
            ) : tab === "commits" ? (
              <Commits d={d} />
            ) : (
              <Checks d={d} />
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function Conversation({ d, drafts, onSubmitted }: { d: PrDetail; drafts: DraftReviewComment[]; onSubmitted: () => void }) {
  const items = [
    ...d.comments.map((c) => ({ key: `c${c.id}`, at: c.created_at, author: c.author, tag: c.id < 0 ? "sending…" : "", html: c.body_html, md: c.body_md })),
    ...d.reviews.map((r) => ({ key: `r${r.id}`, at: r.submitted_at ?? 0, author: r.author, tag: label(r.state), html: r.body_html, md: r.body_md })),
  ].sort((a, b) => a.at - b.at);
  return (
    <div className="mx-auto max-w-3xl space-y-4">
      {d.labels.length > 0 ? (
        <p className="flex flex-wrap gap-1">
          {d.labels.map((l) => <span key={l.id} className="rounded-full border border-border px-2 text-xs">{l.name}</span>)}
        </p>
      ) : null}
      {d.review_requests.length > 0 ? <p className="text-xs text-muted">Review requested from {d.review_requests.join(", ")}</p> : null}
      <section className="rounded border border-border p-4">
        <p className="mb-1 text-xs text-muted"><b className="text-fg">{d.row.author}</b> · {when(d.created_at)}</p>
        <Body html={d.body_html} md={d.body_md} />
      </section>
      {items.map((i) => (
        <section key={i.key} className="rounded border border-border p-4">
          <p className="mb-1 text-xs text-muted">
            <b className="text-fg">{i.author}</b> · {when(i.at)}
            {i.tag ? <span className={`ml-2 rounded border border-border px-1 ${TONE[i.tag.toUpperCase().replaceAll(" ", "_")] ?? ""}`}>{i.tag}</span> : null}
          </p>
          {i.html || i.md ? <Body html={i.html} md={i.md} /> : null}
        </section>
      ))}
      {d.row.comments > d.comments.length ? (
        <p className="text-xs text-muted">{d.row.comments - d.comments.length} more comments are not cached yet.</p>
      ) : null}
      <ReviewForm prId={d.row.id} drafts={drafts} onSubmitted={onSubmitted} />
      <MergeBox d={d} />
    </div>
  );
}

function Commits({ d }: { d: PrDetail }) {
  if (d.commits.length === 0) return <p className="text-muted">No commits cached.</p>;
  return (
    <ul className="mx-auto max-w-3xl divide-y divide-border">
      {d.commits.map((c) => (
        <li key={c.oid} className="flex items-baseline gap-3 py-2">
          <span className="min-w-0 flex-1 truncate">{c.headline || "(not cached)"}</span>
          <span className="text-xs text-muted">{c.author ?? ""} {when(c.authored_at)}</span>
          <code className="text-xs text-muted">{c.oid.slice(0, 7)}</code>
        </li>
      ))}
    </ul>
  );
}

function Checks({ d }: { d: PrDetail }) {
  if (d.checks.length === 0) return <p className="text-muted">No checks reported for the head commit.</p>;
  return (
    <ul className="mx-auto max-w-3xl divide-y divide-border">
      {d.checks.map((c) => {
        const state = c.conclusion ?? c.status;
        return (
          <li key={c.id} className="flex items-center gap-3 py-2">
            <span className={`w-28 text-xs font-semibold ${TONE[state] ?? "text-muted"}`}>{label(state)}</span>
            <span className="flex-1">{c.name}</span>
            {c.url ? <a className="text-xs text-accent" href={c.url} target="_blank" rel="noreferrer">Details</a> : null}
          </li>
        );
      })}
    </ul>
  );
}
