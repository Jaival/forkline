import type { KeyboardEvent } from "react";
import { type LayoutAction, type Pane, viewTitle } from "../lib/layout";
import { ViewHost } from "./ViewHost";

const TAB_KEYS: Record<string, (i: number, n: number) => number> = {
  ArrowLeft: (i, n) => (i - 1 + n) % n,
  ArrowRight: (i, n) => (i + 1) % n,
  Home: () => 0,
  End: (_, n) => n - 1,
};

export function PaneView({
  pane,
  focused,
  split,
  dispatch,
}: {
  pane: Pane;
  focused: boolean;
  split: boolean;
  dispatch: (a: LayoutAction) => void;
}) {
  // ARIA tabs pattern: only the active tab is in the Tab order; arrows,
  // Home and End move between tabs, Delete closes the focused one.
  const onTabKey = (e: KeyboardEvent<HTMLDivElement>, index: number) => {
    const tabId = pane.tabs[index]!.id;
    if (e.key === "Delete") {
      dispatch({ type: "closeTab", paneId: pane.id, tabId });
    } else if (TAB_KEYS[e.key]) {
      const next = TAB_KEYS[e.key]!(index, pane.tabs.length);
      dispatch({ type: "activateTab", paneId: pane.id, tabId: pane.tabs[next]!.id });
      (e.currentTarget.parentElement?.children[next] as HTMLElement | undefined)?.focus();
    } else return;
    e.preventDefault();
  };

  return (
    <section
      className={`flex min-w-0 flex-1 flex-col ${split ? "border-l border-border first:border-l-0" : ""}`}
      onMouseDown={() => !focused && dispatch({ type: "focusPane", paneId: pane.id })}
    >
      <div
        role="tablist"
        className={`flex h-9 shrink-0 items-end gap-px overflow-x-auto border-b border-border bg-panel px-1 ${
          split && focused ? "shadow-[inset_0_2px_0_var(--fl-accent)]" : ""
        }`}
      >
        {pane.tabs.map((t, index) => {
          const active = t.id === pane.activeTabId;
          return (
            <div
              key={t.id}
              role="tab"
              aria-selected={active}
              tabIndex={active ? 0 : -1}
              onKeyDown={(e) => onTabKey(e, index)}
              onMouseDown={(e) => {
                if (e.button === 1) dispatch({ type: "closeTab", paneId: pane.id, tabId: t.id });
                else dispatch({ type: "activateTab", paneId: pane.id, tabId: t.id });
              }}
              className={`group flex h-8 max-w-56 min-w-24 cursor-default items-center gap-2 rounded-t-md px-3 outline-none select-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-inset ${
                active ? "bg-bg text-fg" : "text-muted hover:bg-raised"
              }`}
            >
              <span className="truncate">{viewTitle(t.view)}</span>
              <button
                aria-label="Close tab"
                tabIndex={-1}
                className="ml-auto rounded px-1 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 hover:bg-raised"
                onMouseDown={(e) => e.stopPropagation()}
                onClick={() => dispatch({ type: "closeTab", paneId: pane.id, tabId: t.id })}
              >
                ×
              </button>
            </div>
          );
        })}
        <button
          aria-label="New tab"
          className="mb-1 ml-1 rounded px-2 text-muted hover:bg-raised"
          onClick={() => dispatch({ type: "newTab" })}
        >
          +
        </button>
      </div>
      {/* Tabs stay mounted so switching is a visibility toggle (< 16 ms budget). */}
      <div className="relative min-h-0 flex-1">
        {pane.tabs.map((t) => (
          <div key={t.id} className="absolute inset-0" hidden={t.id !== pane.activeTabId}>
            <ViewHost view={t.view} onNavigate={(view) => dispatch({ type: "navigate", view })} />
          </div>
        ))}
      </div>
    </section>
  );
}
