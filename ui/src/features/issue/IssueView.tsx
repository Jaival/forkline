import { useQuery } from "@tanstack/react-query";
import { commands } from "../../bindings";
import { sanitizeHtml } from "../../lib/html";

export function IssueView({ id }: { id: number }) {
  const issue = useQuery({
    queryKey: ["issues", "detail", id],
    queryFn: () => commands.issueGet(id),
  });

  if (issue.isPending) return <div className="p-6 text-muted">Loading cached item…</div>;
  if (issue.isError) return <div className="p-6" role="alert">Could not load this item from the local cache.</div>;
  return (
    <article className="h-full overflow-auto p-6">
      <p className="mb-2 text-sm text-muted">{issue.data.row.repo} #{issue.data.row.number}</p>
      <h1 className="mb-5 text-xl font-semibold">{issue.data.row.title}</h1>
      {issue.data.body_html ? (
        <div className="max-w-none" dangerouslySetInnerHTML={{ __html: sanitizeHtml(issue.data.body_html) }} />
      ) : <p className="text-muted">No description.</p>}
    </article>
  );
}
