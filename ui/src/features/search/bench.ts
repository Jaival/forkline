import { commands } from "../../bindings";

// Same query set as `crates/fl-search/examples/search_bench.rs`.
const QUERIES = [
  "fixture", "fixture item", "project 42", "gener", "fix*", "PR #7",
  "is:pr fixture", "is:issue is:open fixture", "-is:pr fixture", "author:octocat fixture",
  "repo:forkline-demo/project-007", "repo:forkline-demo/project-007 is:pr item", "is:pr",
  "author:octocat is:open", "label:bug fixture", "assignee:octocat", "fixture -generated",
  '"local data" project', 'NEAR( AND " * -', "zzzznomatch",
];

/** `search_p95_ms`: p95 of the IPC round trip (command + first page) over the query set. */
export async function bench(mark: (name: string, value: number) => Promise<void>): Promise<void> {
  const ms: number[] = [];
  for (let round = 0; round < 11; round++) {
    for (const q of QUERIES) {
      const t = performance.now();
      await commands.search(q, { offset: 0, limit: 50 });
      if (round > 0) ms.push(performance.now() - t);
    }
  }
  ms.sort((a, b) => a - b);
  await mark("search_p95_ms", ms[Math.min(ms.length - 1, Math.floor(ms.length * 0.95))]!);
}
