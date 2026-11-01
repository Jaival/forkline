import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";
import { commands } from "./bindings";
import { listenForDataChanges, queryClient } from "./lib/query";
import { parseLayout } from "./lib/layout";
import { applyTheme, parseTheme } from "./lib/theme";
import { Shell } from "./shell/Shell";
import "./styles.css";

// One IPC call before the first render: saved layout, theme, bench flag.
const boot = await commands.appBoot();
const theme = parseTheme(boot.theme_json);
applyTheme(theme);
void listenForDataChanges();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <Shell initialLayout={parseLayout(boot.layout_json)} initialTheme={theme} />
    </QueryClientProvider>
  </StrictMode>,
);

if (boot.bench) {
  const { runBench } = await import("./bench");
  void runBench();
}
