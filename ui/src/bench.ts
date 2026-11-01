import { commands } from "./bindings";

/** Minimal real-app bench handshake; feature scenarios can append marks. */
export async function runBench(): Promise<void> {
  const started = performance.now();
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
  await commands.benchMark("home_painted", null);
  await commands.benchMark("home_first_paint", performance.now() - started);
  await commands.benchFinish();
}
