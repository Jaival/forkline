import { commands } from "./bindings";
import { afterPaint, waitReady } from "./lib/perf";

/**
 * Real-app bench scenario. Home counts as painted once its rows are in the
 * DOM and the frame has painted, not just the empty shell.
 *
 * The budgeted number, `page_start_to_home`, is `performance.now()` at that
 * point: ms since the page's navigation started. It excludes Tauri / WebView2
 * process startup, which this app cannot speed up. The `home_painted` mark
 * still lets fl-bench report the full time from process start.
 */
export async function runBench(): Promise<void> {
  await waitReady("home");
  await afterPaint();
  const sinceNavigation = performance.now();
  await commands.benchMark("home_painted", null);
  await commands.benchMark("page_start_to_home", sinceNavigation);
  await commands.benchFinish();
}
