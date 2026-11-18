import { afterPaint, resetReady, waitReady } from "../../lib/perf";

/**
 * `open_cached_item_ms`: click on the first Home row until the issue detail
 * (body + thread) is painted. Home must be the active view.
 */
export async function bench(mark: (name: string, value: number) => Promise<void>): Promise<void> {
  await waitReady("home");
  await afterPaint();
  const row = document.querySelector<HTMLElement>("button[data-issue-id]");
  if (!row) throw new Error("bench: no Home row to open");
  const key = `issue:${row.dataset.issueId}`;
  resetReady(key);
  const start = performance.now();
  row.click();
  await waitReady(key);
  await afterPaint();
  await mark("open_cached_item_ms", performance.now() - start);
}
