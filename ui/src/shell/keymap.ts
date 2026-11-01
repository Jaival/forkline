import { useEffect } from "react";
import { activePane, type Layout, type LayoutAction } from "../lib/layout";

// Global shortcuts. Agent 2 extends this into the full keymap
// (j/k, Enter, c, a, l, e, /, Ctrl+K).
export function useKeymap(layout: Layout, dispatch: (a: LayoutAction) => void): void {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.ctrlKey || e.altKey || e.metaKey) return;
      const pane = activePane(layout);
      if (e.key === "t") dispatch({ type: "newTab" });
      else if (e.key === "w") dispatch({ type: "closeTab", paneId: pane.id, tabId: pane.activeTabId });
      else if (e.key === "\\") dispatch({ type: "toggleSplit" });
      else return;
      e.preventDefault();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [layout, dispatch]);
}
