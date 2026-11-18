import { commands } from "./bindings";
import { bench as diffScroll } from "./features/diff/bench";
import { bench as openCachedItem } from "./features/issue/bench";
import { bench as switchTab } from "./features/repo/bench";
import { bench as searchP95 } from "./features/search/bench";
import { afterPaint, waitReady } from "./lib/perf";

/**
 * Real-app bench scenario. Each duration is recorded under its key in
 * `bench/budgets.json`; `fl-bench` compares them.
 *
 * Launch: Home counts as painted once its rows are in the DOM and the frame
 * has painted. `page_start_to_home_ms` is `performance.now()` at that point:
 * ms since the page's navigation started. It excludes Tauri / WebView2
 * process startup, which this app cannot speed up. The `home_painted` mark
 * still lets fl-bench report the full time from process start.
 *
 * Order matters: `openCachedItem` needs Home as the active view, and
 * `switchTab` opens a second tab.
 */
export async function runBench(): Promise<void> {
  await waitReady("home");
  await afterPaint();
  const sinceNavigation = performance.now();
  await commands.benchMark("home_painted", null);
  await commands.benchMark("page_start_to_home_ms", sinceNavigation);

  const mark = async (name: string, value: number) => {
    await commands.benchMark(name, value);
  };
  await openCachedItem(mark);
  await switchTab(mark);
  await searchP95(mark);
  await diffScroll(mark);
  await commands.benchFinish();
}
