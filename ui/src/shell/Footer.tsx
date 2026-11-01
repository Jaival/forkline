import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { commands } from "../bindings";
import type { ThemePref } from "../lib/theme";

const NEXT_THEME: Record<ThemePref, ThemePref> = { system: "light", light: "dark", dark: "system" };

function ago(secs: number): string {
  if (secs < 60) return `${secs} s ago`;
  if (secs < 3600) return `${Math.floor(secs / 60)} min ago`;
  return `${Math.floor(secs / 3600)} h ago`;
}

export function Footer({ theme, onTheme }: { theme: ThemePref; onTheme: (t: ThemePref) => void }) {
  const status = useQuery({ queryKey: ["sync"], queryFn: commands.syncStatus });
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    const t = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(t);
  }, []);

  const s = status.data;
  const synced = s?.last_synced_at ? `Synced ${ago(Math.max(0, now - s.last_synced_at))}` : "Not synced yet";
  return (
    <footer className="col-span-2 flex items-center gap-4 border-t border-border bg-panel px-3 text-xs text-muted">
      <span>{s?.paused ? "Sync paused" : synced}</span>
      {s?.error && <span className="text-closed">{s.error}</span>}
      {!!s?.pending_actions && <span>{s.pending_actions} pending</span>}
      {!!s?.failed_actions && <span className="text-closed">{s.failed_actions} failed</span>}
      <button className="ml-auto hover:text-fg" onClick={() => onTheme(NEXT_THEME[theme])}>
        Theme: {theme}
      </button>
    </footer>
  );
}
