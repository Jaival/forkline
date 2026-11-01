import { commands } from "./bindings";
import { afterPaint, waitReady } from "./lib/perf";

/**
 * Real-app bench scenario. `home_painted` is recorded once the Home feed's
 * rows are in the DOM and the frame has painted, not just the empty shell.
 */
export async function runBench(): Promise<void> {
  const started = performance.now();
  await waitReady("home");
  await afterPaint();
  await commands.benchMark("home_painted", null);
  await commands.benchMark("home_first_paint", performance.now() - started);
  await commands.benchFinish();
}
