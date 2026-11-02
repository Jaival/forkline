import { expect, it } from "vitest";
import { timeAgo } from "./format";

it("formats relative time", () => {
  expect(timeAgo(100, 130)).toBe("now");
  expect(timeAgo(0, 300)).toBe("5m");
  expect(timeAgo(0, 3 * 3600)).toBe("3h");
  expect(timeAgo(0, 12 * 86400)).toBe("12d");
  expect(timeAgo(0, 2 * 365 * 86400)).toBe("2y");
  expect(timeAgo(500, 100)).toBe("now");
});
