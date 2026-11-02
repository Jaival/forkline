/** Short relative time like "5m", "3h", "12d", "2y". `ts` and `now` are unix seconds. */
export function timeAgo(ts: number, now: number = Date.now() / 1000): string {
  const s = Math.max(0, now - ts);
  if (s < 60) return "now";
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  if (s < 86400) return `${Math.floor(s / 3600)}h`;
  if (s < 86400 * 365) return `${Math.floor(s / 86400)}d`;
  return `${Math.floor(s / (86400 * 365))}y`;
}
