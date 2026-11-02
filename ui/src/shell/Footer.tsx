import { useCallback, useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { commands } from "../bindings";
import { SignInDialog } from "../features/auth/SignInDialog";
import { errorText } from "../features/auth/flow";
import type { ThemePref } from "../lib/theme";

const NEXT_THEME: Record<ThemePref, ThemePref> = { system: "light", light: "dark", dark: "system" };

function ago(secs: number): string {
  if (secs < 60) return "just now";
  if (secs < 3600) return `${Math.floor(secs / 60)} min ago`;
  return `${Math.floor(secs / 3600)} h ago`;
}

type Popover = "failed" | "account" | null;

export function Footer({ theme, onTheme }: { theme: ThemePref; onTheme: (t: ThemePref) => void }) {
  const status = useQuery({ queryKey: ["sync"], queryFn: commands.syncStatus });
  const auth = useQuery({ queryKey: ["sync", "auth"], queryFn: commands.authState });
  const failedCount = status.data?.failed_actions ?? 0;
  const [popover, setPopover] = useState<Popover>(null);
  const failed = useQuery({
    queryKey: ["sync", "failed"],
    queryFn: commands.actionFailedList,
    enabled: popover === "failed" && failedCount > 0,
  });
  // The dialog opens by itself while signed out, until the user dismisses it.
  const [dismissed, setDismissed] = useState(false);
  const closeSignIn = useCallback(() => setDismissed(true), []);
  const [actionError, setActionError] = useState<string | null>(null);
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000));
  useEffect(() => {
    // The label has minute resolution, so a 15 s tick is enough.
    const t = setInterval(() => setNow(Math.floor(Date.now() / 1000)), 15_000);
    return () => clearInterval(t);
  }, []);

  const run = (p: Promise<unknown>) => {
    setActionError(null);
    p.catch((e: unknown) => setActionError(errorText(e)));
  };

  const s = status.data;
  const a = auth.data;
  const signedOut = a !== undefined && !a.signed_in;
  const synced = s?.last_synced_at ? `Synced ${ago(Math.max(0, now - s.last_synced_at))}` : "Not synced yet";
  return (
    <footer className="relative col-span-2 flex items-center gap-4 border-t border-border bg-panel px-3 text-xs text-muted">
      {signedOut ? (
        <button className="text-accent hover:underline" onClick={() => setDismissed(false)}>
          Sign in to sync
        </button>
      ) : (
        <button className="hover:text-fg" title="Sync now" onClick={() => run(commands.syncNow())}>
          {s?.paused ? "Sync paused" : synced}
        </button>
      )}
      {s?.error && <span className="truncate text-closed" title={s.error}>{s.error}</span>}
      {!!s?.pending_actions && <span>{s.pending_actions} pending</span>}
      {failedCount > 0 && (
        <button className="text-closed hover:underline" onClick={() => setPopover(popover === "failed" ? null : "failed")}>
          {failedCount} failed
        </button>
      )}
      {actionError && <span className="text-closed">{actionError}</span>}
      <span className="ml-auto" />
      {a?.signed_in && (
        <button className="hover:text-fg" onClick={() => setPopover(popover === "account" ? null : "account")}>
          {a.login ? `@${a.login}` : "Signed in"}
        </button>
      )}
      <button className="hover:text-fg" onClick={() => onTheme(NEXT_THEME[theme])}>
        Theme: {theme}
      </button>

      {popover === "failed" && failedCount > 0 && (
        <div className="absolute bottom-7 left-3 z-40 w-[420px] rounded border border-border bg-panel p-2 text-fg shadow-lg">
          <p className="mb-1 px-1 text-muted">These changes did not reach GitHub and were undone.</p>
          <ul className="max-h-64 divide-y divide-border overflow-auto">
            {failed.data?.map((f) => (
              <li key={f.id} className="flex items-start gap-2 px-1 py-1.5">
                <div className="min-w-0 flex-1">
                  <div className="truncate">{f.summary}</div>
                  <div className="truncate text-closed" title={f.error}>{f.error}</div>
                </div>
                <button className="text-accent hover:underline" onClick={() => run(commands.actionRetry(f.id))}>
                  Retry
                </button>
                <button className="text-muted hover:text-fg" onClick={() => run(commands.actionDiscard(f.id))}>
                  Discard
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
      {popover === "account" && a?.signed_in && (
        <div className="absolute bottom-7 right-3 z-40 w-64 rounded border border-border bg-panel p-3 text-fg shadow-lg">
          {a.env_override ? (
            <p className="text-muted">Signed in with FORKLINE_GITHUB_TOKEN. Unset it to sign out.</p>
          ) : (
            <button
              className="text-closed hover:underline"
              onClick={() => {
                setPopover(null);
                run(commands.authSignOut());
              }}
            >
              Sign out
            </button>
          )}
        </div>
      )}
      {signedOut && !dismissed && <SignInDialog auth={a} onClose={closeSignIn} />}
    </footer>
  );
}
