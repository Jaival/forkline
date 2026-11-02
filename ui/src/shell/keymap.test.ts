import { describe, expect, it } from "vitest";
import { cycleTabId } from "./keymap";

const pane = (active: string) => ({
  id: "p",
  activeTabId: active,
  tabs: ["a", "b", "c"].map((id) => ({ id, view: { kind: "home" as const } })),
});

describe("cycleTabId", () => {
  it("moves forward and wraps", () => {
    expect(cycleTabId(pane("a"), 1)).toBe("b");
    expect(cycleTabId(pane("c"), 1)).toBe("a");
  });

  it("moves backward and wraps", () => {
    expect(cycleTabId(pane("b"), -1)).toBe("a");
    expect(cycleTabId(pane("a"), -1)).toBe("c");
  });
});
