// Tabs + split panes. Pure functions over an immutable layout (unit tested).
// The layout is saved as JSON in SQLite (ui_layout) and restored on launch.

export type ViewRef =
  | { kind: "home" }
  | { kind: "inbox" }
  | { kind: "pulls" }
  | { kind: "issues" }
  | { kind: "reviews" }
  | { kind: "starred" }
  | { kind: "repos" }
  | { kind: "repo"; repoId: number }
  | { kind: "issue"; id: number }
  | { kind: "pr"; id: number }
  | { kind: "search"; q: string };

export type Tab = { id: string; view: ViewRef };
export type Pane = { id: string; tabs: Tab[]; activeTabId: string };
export type Layout = { v: 1; panes: Pane[]; activePaneId: string };

let counter = 0;
export const newId = (): string => `${Date.now().toString(36)}-${(counter++).toString(36)}`;

const tab = (view: ViewRef): Tab => ({ id: newId(), view });

export function defaultLayout(): Layout {
  const t = tab({ kind: "home" });
  const pane: Pane = { id: newId(), tabs: [t], activeTabId: t.id };
  return { v: 1, panes: [pane], activePaneId: pane.id };
}

const VIEW_KINDS = new Set([
  "home", "inbox", "pulls", "issues", "reviews", "starred", "repos", "repo", "issue", "pr", "search",
]);

function isView(v: unknown): v is ViewRef {
  if (typeof v !== "object" || v === null) return false;
  const o = v as Record<string, unknown>;
  if (typeof o.kind !== "string" || !VIEW_KINDS.has(o.kind)) return false;
  if (o.kind === "repo") return typeof o.repoId === "number";
  if (o.kind === "issue" || o.kind === "pr") return typeof o.id === "number";
  if (o.kind === "search") return typeof o.q === "string";
  return true;
}

/** Validate saved JSON (boundary). Falls back to the default layout. */
export function parseLayout(json: string | null): Layout {
  if (!json) return defaultLayout();
  try {
    const l = JSON.parse(json) as Layout;
    const ok =
      l.v === 1 &&
      Array.isArray(l.panes) &&
      l.panes.length >= 1 &&
      l.panes.length <= 2 &&
      l.panes.every(
        (p) =>
          typeof p.id === "string" &&
          Array.isArray(p.tabs) &&
          p.tabs.length > 0 &&
          p.tabs.every((t) => typeof t.id === "string" && isView(t.view)) &&
          p.tabs.some((t) => t.id === p.activeTabId),
      ) &&
      l.panes.some((p) => p.id === l.activePaneId);
    return ok ? l : defaultLayout();
  } catch {
    return defaultLayout();
  }
}

const mapPane = (l: Layout, paneId: string, f: (p: Pane) => Pane): Layout => ({
  ...l,
  panes: l.panes.map((p) => (p.id === paneId ? f(p) : p)),
});

export const activePane = (l: Layout): Pane => l.panes.find((p) => p.id === l.activePaneId) ?? l.panes[0]!;

export const activeTab = (p: Pane): Tab => p.tabs.find((t) => t.id === p.activeTabId) ?? p.tabs[0]!;

export type LayoutAction =
  | { type: "newTab"; view?: ViewRef }
  | { type: "closeTab"; paneId: string; tabId: string }
  | { type: "activateTab"; paneId: string; tabId: string }
  | { type: "navigate"; view: ViewRef }
  | { type: "toggleSplit" }
  | { type: "focusPane"; paneId: string };

export function reduceLayout(l: Layout, a: LayoutAction): Layout {
  switch (a.type) {
    case "newTab": {
      const t = tab(a.view ?? { kind: "home" });
      return mapPane(l, l.activePaneId, (p) => ({ ...p, tabs: [...p.tabs, t], activeTabId: t.id }));
    }
    case "closeTab": {
      const pane = l.panes.find((p) => p.id === a.paneId);
      const idx = pane ? pane.tabs.findIndex((t) => t.id === a.tabId) : -1;
      if (!pane || idx < 0) return l;
      const tabs = pane.tabs.filter((t) => t.id !== a.tabId);
      if (tabs.length === 0) {
        // Last tab of a split pane closes the pane; last tab overall becomes Home.
        if (l.panes.length > 1) {
          const panes = l.panes.filter((p) => p.id !== a.paneId);
          return { ...l, panes, activePaneId: panes[0]!.id };
        }
        const t = tab({ kind: "home" });
        return mapPane(l, a.paneId, (p) => ({ ...p, tabs: [t], activeTabId: t.id }));
      }
      const activeTabId =
        pane.activeTabId === a.tabId ? tabs[Math.min(idx, tabs.length - 1)]!.id : pane.activeTabId;
      return mapPane(l, a.paneId, (p) => ({ ...p, tabs, activeTabId }));
    }
    case "activateTab":
      return { ...mapPane(l, a.paneId, (p) => ({ ...p, activeTabId: a.tabId })), activePaneId: a.paneId };
    case "navigate":
      return mapPane(l, l.activePaneId, (p) => ({
        ...p,
        tabs: p.tabs.map((t) => (t.id === p.activeTabId ? { ...t, view: a.view } : t)),
      }));
    case "toggleSplit": {
      if (l.panes.length > 1) {
        // Merge the second pane's tabs into the first.
        const [first, second] = l.panes as [Pane, Pane];
        return { ...l, panes: [{ ...first, tabs: [...first.tabs, ...second.tabs] }], activePaneId: first.id };
      }
      const t = tab(activeTab(activePane(l)).view);
      const pane: Pane = { id: newId(), tabs: [t], activeTabId: t.id };
      return { ...l, panes: [...l.panes, pane], activePaneId: pane.id };
    }
    case "focusPane":
      return l.panes.some((p) => p.id === a.paneId) ? { ...l, activePaneId: a.paneId } : l;
  }
}

export function viewTitle(v: ViewRef): string {
  switch (v.kind) {
    case "home": return "Home";
    case "inbox": return "Inbox";
    case "pulls": return "Pull requests";
    case "issues": return "Issues";
    case "reviews": return "Review requests";
    case "starred": return "Starred";
    case "repos": return "Repositories";
    case "repo": return `Repository ${v.repoId}`;
    case "issue": return `Issue ${v.id}`;
    case "pr": return `PR ${v.id}`;
    case "search": return `Search: ${v.q}`;
  }
}
