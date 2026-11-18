import { describe, expect, it } from "vitest";
import { filterItems } from "./paletteState";

describe("filterItems", () => {
  const items = [{ label: "Pull requests" }, { label: "acme/rocket" }, { label: "Inbox" }];

  it("matches case-insensitively on substrings", () => {
    expect(filterItems(items, "PULL")).toEqual([{ label: "Pull requests" }]);
    expect(filterItems(items, "ock")).toEqual([{ label: "acme/rocket" }]);
  });

  it("keeps everything for an empty query", () => {
    expect(filterItems(items, "  ")).toHaveLength(3);
  });
});
