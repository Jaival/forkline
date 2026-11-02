import { createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterPaint } from "../../lib/perf";
import { DiffLines, type DiffLinesHandle } from "./DiffLines";
import { makeFixtureFile } from "./fixture";
import { buildRows } from "./rows";

const SCROLL_MS = 2000;
const PX_PER_FRAME = 40;

/**
 * Mounts a 10k-line fixture diff, scrolls it for two seconds and reports the
 * frame rate. Frames are counted with requestAnimationFrame, so a slow render
 * lowers the number.
 */
export async function bench(mark: (name: string, value: number) => Promise<void>): Promise<void> {
  const host = document.createElement("div");
  host.style.cssText = "position:fixed;inset:0;z-index:9999;background:var(--fl-bg)";
  document.body.append(host);
  const root = createRoot(host);
  const ref = { current: null as DiffLinesHandle | null };
  const { rows } = buildRows(makeFixtureFile("src/generated/big.rs", 10_000), [], [], null);
  root.render(createElement(DiffLines, { ref: (h: DiffLinesHandle | null) => { ref.current = h; }, rows }));
  await afterPaint();

  const el = ref.current?.scrollEl;
  if (!el) throw new Error("diff scroll bench: viewer did not mount");
  let frames = 0;
  const start = performance.now();
  await new Promise<void>((resolve) => {
    const tick = (now: number) => {
      frames++;
      el.scrollTop += PX_PER_FRAME;
      if (now - start < SCROLL_MS) requestAnimationFrame(tick);
      else resolve();
    };
    requestAnimationFrame(tick);
  });
  const seconds = (performance.now() - start) / 1000;
  await mark("diff_scroll_fps", frames / seconds);
  root.unmount();
  host.remove();
}
