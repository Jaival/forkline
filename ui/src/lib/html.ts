import DOMPurify from "dompurify";
import MarkdownIt from "markdown-it";
import type { StateCore } from "markdown-it";

/**
 * Markdown / HTML rendering shared by issue, PR and diff views.
 *
 * - `renderBody(html, md)`: render a GitHub body. Prefer GitHub's own
 *   `bodyHTML`; fall back to markdown for local comments that have none yet.
 * - `renderMarkdown(md)`: markdown to sanitized HTML (GFM task lists, tables,
 *   fenced code, autolinks; raw HTML in the source is escaped).
 * - `sanitizeHtml(html)`: DOMPurify pass for any HTML that reaches the DOM.
 *
 * Output is meant for `dangerouslySetInnerHTML` and is always sanitized.
 */

/** GitHub's `bodyHTML` is rendered after sanitizing. */
export function sanitizeHtml(html: string): string {
  return DOMPurify.sanitize(html, { USE_PROFILES: { html: true } });
}

const md = new MarkdownIt({ html: false, linkify: true, breaks: false });

// GFM task lists: "- [ ] x" / "- [x] x" become disabled checkboxes. Runs after
// text_join so "[ ]" is one text token.
md.core.ruler.push("task-lists", (state: StateCore) => {
  const t = state.tokens;
  for (let i = 2; i < t.length; i++) {
    const inline = t[i]!;
    if (inline.type !== "inline" || t[i - 1]!.type !== "paragraph_open" || t[i - 2]!.type !== "list_item_open") continue;
    const m = /^\[([ xX])\] /.exec(inline.content);
    const first = inline.children?.[0];
    if (!m || first?.type !== "text") continue;
    first.content = first.content.slice(4);
    const box = new state.Token("html_inline", "", 0);
    box.content = `<input type="checkbox" disabled${m[1] === " " ? "" : " checked"}> `;
    inline.children!.unshift(box);
    t[i - 2]!.attrJoin("class", "task-list-item");
  }
});

/** Unsanitized markdown render. Only exported for tests; use `renderMarkdown`. */
export function markdownToHtml(src: string): string {
  return md.render(src);
}

export function renderMarkdown(src: string): string {
  return sanitizeHtml(markdownToHtml(src));
}

export function renderBody(html: string | null, mdSrc: string | null): string {
  if (html) return sanitizeHtml(html);
  return mdSrc ? renderMarkdown(mdSrc) : "";
}
