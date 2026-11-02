/** Search snippets mark matches with U+E000 (start) and U+E001 (end). */
export type Segment = { text: string; hit: boolean };

export function splitHighlight(snippet: string): Segment[] {
  const out: Segment[] = [];
  let hit = false;
  for (const part of snippet.split(/([\uE000\uE001])/)) {
    if (part === "\uE000") hit = true;
    else if (part === "\uE001") hit = false;
    else if (part) out.push({ text: part, hit });
  }
  return out;
}
