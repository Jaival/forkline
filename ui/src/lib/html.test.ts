import { describe, expect, it } from "vitest";
import { markdownToHtml } from "./html";

describe("markdownToHtml", () => {
  it("renders task lists as disabled checkboxes", () => {
    const html = markdownToHtml("- [x] done\n- [ ] todo\n- plain");
    expect(html).toContain('<li class="task-list-item"><input type="checkbox" disabled checked> done</li>');
    expect(html).toContain('<li class="task-list-item"><input type="checkbox" disabled> todo</li>');
    expect(html).toContain("<li>plain</li>");
  });

  it("renders fenced code and escapes raw html", () => {
    const html = markdownToHtml("```rust\nfn main() {}\n```\n\n<script>alert(1)</script>");
    expect(html).toContain('<pre><code class="language-rust">fn main() {}');
    expect(html).not.toContain("<script>");
  });
});
