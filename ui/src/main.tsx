import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { commands } from "./bindings";
import { interceptLinks } from "./lib/links";
import { listenForDataChanges, queryClient } from "./lib/query";
import { parseLayout } from "./lib/layout";
import { applyTheme, parseTheme } from "./lib/theme";
import { Shell } from "./shell/Shell";
import "./styles.css";

const root = document.getElementById("root")!;

// One IPC call before the first render: saved layout, theme, bench flag.
// If it fails there is nothing to render, so show why instead of a blank window.
const boot = await commands.appBoot().catch((e: unknown) => {
  root.textContent = `Forkline could not start: ${JSON.stringify(e)}`;
  throw e;
});
const theme = parseTheme(boot.theme_json);
applyTheme(theme);
interceptLinks();
void listenForDataChanges();

// Bench runs always start from the default layout so they measure Home.
const initialLayout = parseLayout(boot.bench ? null : boot.layout_json);

createRoot(root).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <Shell initialLayout={initialLayout} initialTheme={theme} persistLayout={!boot.bench} />
    </QueryClientProvider>
  </StrictMode>,
);

if (boot.bench) {
  const { runBench } = await import("./bench");
  void runBench();
}
