import { openUrl } from "@tauri-apps/plugin-opener";

/**
 * Keep the app window on the app. Web and mail links (e.g. in rendered issue
 * bodies) open in the system browser; anything else is blocked. In-page `#`
 * anchors still scroll normally.
 */
export function interceptLinks(): void {
  const onClick = (e: MouseEvent) => {
    const a = e.target instanceof Element ? e.target.closest("a[href]") : null;
    if (!(a instanceof HTMLAnchorElement) || a.getAttribute("href")?.startsWith("#")) return;
    e.preventDefault();
    const url = new URL(a.href);
    if (url.origin !== location.origin && /^(https?|mailto):$/.test(url.protocol)) {
      openUrl(url.href).catch(console.error);
    }
  };
  document.addEventListener("click", onClick);
  document.addEventListener("auxclick", onClick);
}
