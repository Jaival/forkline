import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { commands, type AuthState, type DeviceCode } from "../../bindings";
import { queryClient } from "../../lib/query";
import { errorText, nextStep } from "./flow";

/**
 * GitHub device-flow sign-in. Shown by the footer when no token exists or
 * GitHub rejected it. Without a build-time client ID it explains the
 * `FORKLINE_GITHUB_TOKEN` dev override instead.
 */
export function SignInDialog({ auth, onClose }: { auth: AuthState; onClose: () => void }) {
  const [code, setCode] = useState<DeviceCode | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Poll at the server's interval until GitHub answers.
  useEffect(() => {
    if (!code) return;
    let cancelled = false;
    let timer = 0;
    const tick = async () => {
      try {
        const step = nextStep(await commands.authPoll());
        if (cancelled) return;
        if (step.kind === "wait") timer = window.setTimeout(() => void tick(), step.ms);
        else if (step.kind === "done") {
          await queryClient.invalidateQueries({ queryKey: ["sync"] });
          onClose();
        } else {
          setCode(null);
          setMessage(step.message);
        }
      } catch (e) {
        if (cancelled) return;
        setCode(null);
        setMessage(errorText(e));
      }
    };
    timer = window.setTimeout(() => void tick(), code.interval * 1000);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [code, onClose]);

  const start = async () => {
    setBusy(true);
    setMessage(null);
    try {
      const c = await commands.authStart();
      setCode(c);
      await openUrl(c.verification_uri);
    } catch (e) {
      setMessage(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40" role="dialog" aria-modal="true" aria-labelledby="signin-title">
      <div className="w-[440px] rounded-lg border border-border bg-panel p-5 text-sm text-fg shadow-xl">
        <h2 id="signin-title" className="mb-3 text-base font-semibold">Sign in to GitHub</h2>
        {!auth.client_id_configured ? (
          <div className="space-y-2">
            <p>This build has no GitHub OAuth client ID, so sign-in through GitHub is not available yet.</p>
            <p>
              For development, set <code className="rounded bg-raised px-1">FORKLINE_GITHUB_TOKEN</code> to a
              personal access token (scopes: <code>repo</code>, <code>read:org</code>, <code>notifications</code>)
              and restart Forkline.
            </p>
          </div>
        ) : code ? (
          <div className="space-y-3">
            <p>Enter this code on GitHub. The page opened in your browser.</p>
            <div className="flex items-center gap-3">
              <span className="select-all rounded bg-raised px-3 py-2 font-mono text-lg tracking-widest">{code.user_code}</span>
              <button className="text-accent hover:underline" onClick={() => void navigator.clipboard.writeText(code.user_code)}>
                Copy
              </button>
            </div>
            <p className="text-muted">
              Waiting for approval…{" "}
              <button className="text-accent hover:underline" onClick={() => void openUrl(code.verification_uri)}>
                Open the page again
              </button>
            </p>
          </div>
        ) : (
          <div className="space-y-3">
            <p>Forkline keeps a local copy of your repositories, issues, pull requests and inbox.</p>
            <button
              className="rounded bg-accent px-3 py-1.5 font-medium text-white disabled:opacity-50"
              disabled={busy}
              onClick={() => void start()}
            >
              Sign in with GitHub
            </button>
          </div>
        )}
        {message && <p role="alert" className="mt-3 text-closed">{message}</p>}
        <div className="mt-4 flex justify-end">
          <button className="text-muted hover:text-fg" onClick={onClose}>
            {code ? "Cancel" : "Not now"}
          </button>
        </div>
      </div>
    </div>
  );
}
