import { type LayoutAction, type Pane, viewTitle } from "../lib/layout";
import { ViewHost } from "./ViewHost";

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
        {pane.tabs.map((t) => {
          const active = t.id === pane.activeTabId;
          return (
            <div
              key={t.id}
              role="tab"
              aria-selected={active}
              onMouseDown={(e) => {
                if (e.button === 1) dispatch({ type: "closeTab", paneId: pane.id, tabId: t.id });
                else dispatch({ type: "activateTab", paneId: pane.id, tabId: t.id });
              }}
              className={`group flex h-8 max-w-56 min-w-24 cursor-default items-center gap-2 rounded-t-md px-3 select-none ${
                active ? "bg-bg text-fg" : "text-muted hover:bg-raised"
              }`}
            >
              <span className="truncate">{viewTitle(t.view)}</span>
              <button
                aria-label="Close tab"
                className="ml-auto rounded px-1 opacity-0 group-hover:opacity-100 hover:bg-raised"
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
