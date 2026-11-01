import DOMPurify from "dompurify";
import MarkdownIt from "markdown-it";

/** GitHub's `bodyHTML` is rendered after sanitizing. */
export function sanitizeHtml(html: string): string {
  return DOMPurify.sanitize(html, { USE_PROFILES: { html: true } });
}

// Only for the composer preview and optimistic comments (no bodyHTML yet).
// Task lists are added by agent 3.
const md = new MarkdownIt({ html: false, linkify: true, breaks: false });

export function renderMarkdown(src: string): string {
  return sanitizeHtml(md.render(src));
}
