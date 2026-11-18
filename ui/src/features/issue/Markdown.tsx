import { useMemo } from "react";
import { renderBody } from "../../lib/html";

const STYLE = [
  "break-words leading-relaxed",
  "[&_a]:text-accent [&_a]:underline",
  "[&_p]:my-2 [&_h1]:my-3 [&_h1]:text-lg [&_h1]:font-semibold [&_h2]:my-3 [&_h2]:text-base [&_h2]:font-semibold [&_h3]:my-2 [&_h3]:font-semibold",
  "[&_ul]:my-2 [&_ul]:list-disc [&_ul]:pl-6 [&_ol]:my-2 [&_ol]:list-decimal [&_ol]:pl-6",
  "[&_li.task-list-item]:list-none [&_li.task-list-item]:-ml-5 [&_li.task-list-item_input]:mr-1",
  "[&_blockquote]:my-2 [&_blockquote]:border-l-4 [&_blockquote]:border-border [&_blockquote]:pl-3 [&_blockquote]:text-muted",
  "[&_code]:rounded [&_code]:bg-raised [&_code]:px-1 [&_code]:font-mono [&_code]:text-[12px]",
  "[&_pre]:my-2 [&_pre]:overflow-auto [&_pre]:rounded [&_pre]:bg-raised [&_pre]:p-3 [&_pre_code]:bg-transparent [&_pre_code]:p-0",
  "[&_table]:my-2 [&_td]:border [&_td]:border-border [&_td]:px-2 [&_td]:py-1 [&_th]:border [&_th]:border-border [&_th]:px-2 [&_th]:py-1",
  "[&_img]:max-w-full",
].join(" ");

/** GitHub body (`bodyHTML`, or markdown when there is none) as sanitized HTML. */
export function Markdown({ html, md }: { html: string | null; md: string | null }) {
  const out = useMemo(() => renderBody(html, md), [html, md]);
  if (!out) return <p className="text-muted">No description.</p>;
  return <div className={STYLE} dangerouslySetInnerHTML={{ __html: out }} />;
}
