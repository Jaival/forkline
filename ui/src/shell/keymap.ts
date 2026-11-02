import { useEffect } from "react";
import { isTypingTarget } from "../lib/listNav";
import { activePane, type Layout, type LayoutAction, type Pane } from "../lib/layout";
import { openPalette } from "../features/palette/paletteState";

// Global keymap.
//
//   Ctrl+K            command palette
//   Ctrl+T / Ctrl+W   new / close tab
//   Ctrl+\            split / unsplit
//   Ctrl+Tab          next tab (Ctrl+Shift+Tab: previous)
//   /                 focus the global search box (element id "global-search")
//   j / k             with nothing focused, move into the active view's list
//                     (the element marked `data-list-nav`, see lib/listNav.ts)
//
// Lists handle j / k / Enter themselves through `useListNav`.

/** Id of the tab `step` places away from the active one (wraps). */
export function cycleTabId(pane: Pane, step: 1 | -1): string {
  const i = pane.tabs.findIndex((t) => t.id === pane.activeTabId);
  return pane.tabs[(i + step + pane.tabs.length) % pane.tabs.length]!.id;
}

/** The visible list of the active view, if any. */
function visibleList(): HTMLElement | undefined {
  return [...document.querySelectorAll<HTMLElement>("[data-list-nav]")].find((el) => el.offsetParent !== null);
}

export function useKeymap(layout: Layout, dispatch: (a: LayoutAction) => void): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey || e.metaKey) return;
      const pane = activePane(layout);
      if (e.ctrlKey) {
        const key = e.key.toLowerCase();
        if (key === "t") dispatch({ type: "newTab" });
        else if (key === "w") dispatch({ type: "closeTab", paneId: pane.id, tabId: pane.activeTabId });
        else if (key === "\\") dispatch({ type: "toggleSplit" });
        else if (key === "k") openPalette();
        else if (e.key === "Tab") {
          dispatch({ type: "activateTab", paneId: pane.id, tabId: cycleTabId(pane, e.shiftKey ? -1 : 1) });
        } else return;
        e.preventDefault();
        return;
      }
      if (e.shiftKey || isTypingTarget(e.target)) return;
      if (e.key === "/") {
        const box = document.getElementById("global-search");
        if (!box) return;
        box.focus();
      } else if (e.key === "j" || e.key === "k") {
        // Nothing list-like has focus: hand the key to the active list.
        const list = visibleList();
        if (!list || list.contains(e.target as Node)) return;
        list.focus();
        list.dispatchEvent(new KeyboardEvent("keydown", { key: e.key, bubbles: true, cancelable: true }));
      } else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [layout, dispatch]);
}
