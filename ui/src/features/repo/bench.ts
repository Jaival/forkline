import { activePane } from "../../lib/layout";
import { afterPaint, waitReady } from "../../lib/perf";
import { driver } from "../../shell/driver";

const SWITCHES = 20;

/**
 * `switch_tab_ms`: median time from activating an already-open tab to the
 * next painted frame. Tabs stay mounted, so this is a visibility toggle.
 */
export async function bench(mark: (name: string, value: number) => Promise<void>): Promise<void> {
  const { dispatch } = driver;
  if (!dispatch) throw new Error("bench: shell driver is not ready");
  dispatch({ type: "newTab", view: { kind: "repos" } });
  await waitReady("repos");
  await afterPaint();

  const pane = activePane(driver.layout!);
  const [first, second] = [pane.tabs[0]!.id, pane.activeTabId];
  const times: number[] = [];
  for (let i = 0; i < SWITCHES; i++) {
    const t0 = performance.now();
    dispatch({ type: "activateTab", paneId: pane.id, tabId: i % 2 === 0 ? first : second });
    await afterPaint();
    times.push(performance.now() - t0);
  }
  times.sort((a, b) => a - b);
  await mark("switch_tab_ms", times[Math.floor(times.length / 2)]!);
}
