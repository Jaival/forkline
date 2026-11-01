import { describe, expect, it } from "vitest";
import { activePane, activeTab, defaultLayout, parseLayout, reduceLayout } from "./layout";

describe("layout", () => {
  it("opens and closes tabs", () => {
    let l = defaultLayout();
    l = reduceLayout(l, { type: "newTab", view: { kind: "inbox" } });
    const p = activePane(l);
    expect(p.tabs).toHaveLength(2);
    expect(activeTab(p).view).toEqual({ kind: "inbox" });
    l = reduceLayout(l, { type: "closeTab", paneId: p.id, tabId: p.activeTabId });
    expect(activePane(l).tabs).toHaveLength(1);
    expect(activeTab(activePane(l)).view).toEqual({ kind: "home" });
  });

  it("closing the last tab leaves a Home tab", () => {
    let l = reduceLayout(defaultLayout(), { type: "navigate", view: { kind: "issues" } });
    const p = activePane(l);
    l = reduceLayout(l, { type: "closeTab", paneId: p.id, tabId: p.activeTabId });
    expect(activePane(l).tabs).toHaveLength(1);
    expect(activeTab(activePane(l)).view.kind).toBe("home");
  });

  it("splits and unsplits", () => {
    let l = reduceLayout(defaultLayout(), { type: "toggleSplit" });
    expect(l.panes).toHaveLength(2);
    expect(l.activePaneId).toBe(l.panes[1]!.id);
    l = reduceLayout(l, { type: "toggleSplit" });
    expect(l.panes).toHaveLength(1);
    expect(l.panes[0]!.tabs).toHaveLength(2);
  });

  it("closing the last tab of a split pane closes the pane", () => {
    let l = reduceLayout(defaultLayout(), { type: "toggleSplit" });
    const second = l.panes[1]!;
    l = reduceLayout(l, { type: "closeTab", paneId: second.id, tabId: second.activeTabId });
    expect(l.panes).toHaveLength(1);
  });

  it("round-trips through JSON and rejects bad input", () => {
    const l = reduceLayout(defaultLayout(), { type: "newTab", view: { kind: "issue", id: 42 } });
    expect(parseLayout(JSON.stringify(l))).toEqual(l);
    expect(parseLayout("{bad").panes).toHaveLength(1);
    expect(parseLayout(JSON.stringify({ v: 1, panes: [], activePaneId: "x" })).panes).toHaveLength(1);
    const badView = { ...l, panes: [{ ...l.panes[0]!, tabs: [{ id: "t", view: { kind: "issue" } }] }] };
    expect(parseLayout(JSON.stringify(badView))).not.toEqual(badView);
  });
});
