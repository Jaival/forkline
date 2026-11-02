import { expect, test } from "vitest";
import { splitHighlight } from "./highlight";

test("splits marked matches", () => {
  expect(splitHighlight("a \uE000b\uE001 c")).toEqual([
    { text: "a ", hit: false },
    { text: "b", hit: true },
    { text: " c", hit: false },
  ]);
});

test("plain text and empty input", () => {
  expect(splitHighlight("plain")).toEqual([{ text: "plain", hit: false }]);
  expect(splitHighlight("")).toEqual([]);
});

test("unterminated match stays highlighted to the end", () => {
  expect(splitHighlight("x\uE000y")).toEqual([
    { text: "x", hit: false },
    { text: "y", hit: true },
  ]);
});
