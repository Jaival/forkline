import { describe, expect, it } from "vitest";
import { languageShares, sparklinePoints } from "./sparkline";

describe("sparklinePoints", () => {
  it("scales the series into the box, low values at the bottom", () => {
    expect(sparklinePoints([0, 5, 10], 100, 20)).toBe("0.0,20.0 50.0,10.0 100.0,0.0");
  });

  it("handles flat and too-short series", () => {
    expect(sparklinePoints([3, 3], 10, 10)).toBe("0.0,10.0 10.0,10.0");
    expect(sparklinePoints([1], 10, 10)).toBe("");
  });
});

describe("languageShares", () => {
  it("returns percentages, largest first", () => {
    expect(languageShares([{ name: "A", bytes: 1 }, { name: "B", bytes: 3 }])).toEqual([
      { name: "B", percent: 75 },
      { name: "A", percent: 25 },
    ]);
    expect(languageShares([])).toEqual([]);
  });
});
