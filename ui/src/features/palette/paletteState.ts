// Open signal and filtering for the command palette. keymap.ts fires the
// signal, the <Palette> component (mounted once, in the sidebar) listens.

import type { ViewRef } from "../../lib/layout";

const EVENT = "fl:open-palette";

export const openPalette = (): void => void window.dispatchEvent(new Event(EVENT));

export function onOpenPalette(f: () => void): () => void {
  window.addEventListener(EVENT, f);
  return () => window.removeEventListener(EVENT, f);
}

export type PaletteItem = { id: string; label: string; hint: string; view: ViewRef };

/** Case-insensitive substring filter on the label. Empty query keeps all. */
export function filterItems<T extends { label: string }>(items: T[], query: string): T[] {
  const q = query.trim().toLowerCase();
  return q ? items.filter((i) => i.label.toLowerCase().includes(q)) : items;
}
