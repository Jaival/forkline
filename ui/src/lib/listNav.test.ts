import { describe, expect, it } from "vitest";
import { nextIndex } from "./listNav";

describe("nextIndex", () => {
  it("moves with j / k and clamps at both ends", () => {
    expect(nextIndex("j", -1, 3)).toBe(0);
    expect(nextIndex("j", 2, 3)).toBe(2);
    expect(nextIndex("k", 2, 3)).toBe(1);
    expect(nextIndex("k", 0, 3)).toBe(0);
    expect(nextIndex("k", -1, 3)).toBe(0);
  });

  it("supports arrows, Home and End", () => {
    expect(nextIndex("ArrowDown", 0, 3)).toBe(1);
    expect(nextIndex("ArrowUp", 1, 3)).toBe(0);
    expect(nextIndex("Home", 2, 3)).toBe(0);
    expect(nextIndex("End", 0, 3)).toBe(2);
  });

  it("ignores other keys and empty lists", () => {
    expect(nextIndex("x", 0, 3)).toBeNull();
    expect(nextIndex("j", -1, 0)).toBeNull();
  });
});
