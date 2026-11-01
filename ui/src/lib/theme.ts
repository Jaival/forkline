import { getCurrentWindow } from "@tauri-apps/api/window";

export type ThemePref = "system" | "light" | "dark";

/** Validate the saved setting (boundary). */
export function parseTheme(json: string | null): ThemePref {
  if (!json) return "system";
  try {
    const v: unknown = JSON.parse(json);
    return v === "light" || v === "dark" ? v : "system";
  } catch {
    return "system";
  }
}

/**
 * "system" removes the attribute so CSS follows `prefers-color-scheme`
 * (WebView2 follows the Windows setting). Also sets the native title bar.
 */
export function applyTheme(pref: ThemePref): void {
  const root = document.documentElement;
  if (pref === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", pref);
  void getCurrentWindow().setTheme(pref === "system" ? null : pref);
}
