import { expect, it } from "vitest";
import type { CommentRow, TimelineEvent } from "../../bindings";
import { buildTimeline, describeEvent } from "./timeline";

const comment = (id: number, created_at: number): CommentRow =>
  ({ id, author: "a", body_html: null, body_md: "x", created_at });
const event = (id: number, created_at: number, kind = "closed", payload_json: string | null = null): TimelineEvent =>
  ({ id, kind, actor: "a", created_at, payload_json });

it("merges by time, comments first on ties", () => {
  const items = buildTimeline([comment(1, 20), comment(-1, 30)], [event(1, 10), event(2, 20)]);
  expect(items.map((i) => i.key)).toEqual(["e1", "c1", "e2", "c-1"]);
});

it("describes events and survives bad payloads", () => {
  expect(describeEvent(event(1, 0, "labeled", '{"label":"bug"}'))).toBe("added the bug label");
  expect(describeEvent(event(1, 0, "labeled", "not json"))).toBe("added the label");
  expect(describeEvent(event(1, 0, "head_ref_force_pushed"))).toBe("head ref force pushed");
});
