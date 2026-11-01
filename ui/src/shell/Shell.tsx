import { useEffect, useReducer, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { commands } from "../bindings";
import { type Layout, reduceLayout } from "../lib/layout";
import { applyTheme, type ThemePref } from "../lib/theme";
import { driver } from "./driver";
import { Footer } from "./Footer";
import { useKeymap } from "./keymap";
import { PaneView } from "./PaneView";
import { Sidebar } from "./Sidebar";

const saveLayout = (l: Layout): Promise<void> =>
  commands.layoutSave(JSON.stringify(l)).then(() => undefined, console.error);

export function Shell({
  initialLayout,
  initialTheme,
  persistLayout,
}: {
  initialLayout: Layout;
  initialTheme: ThemePref;
  /** False in bench mode, so a bench run never overwrites the saved layout. */
  persistLayout: boolean;
}) {
  const [layout, dispatch] = useReducer(reduceLayout, initialLayout);
  const [theme, setTheme] = useState(initialTheme);
  const latest = useRef(layout);

  useEffect(() => {
    driver.dispatch = dispatch;
    driver.layout = layout;
    latest.current = layout;
  }, [dispatch, layout]);

  // Save tabs + panes shortly after each change.
  useEffect(() => {
    if (!persistLayout) return;
    const t = setTimeout(() => void saveLayout(layout), 300);
    return () => clearTimeout(t);
  }, [layout, persistLayout]);

  // Flush the last change on close; the window closes once the save resolves.
  useEffect(() => {
    if (!persistLayout) return;
    const unlisten = getCurrentWindow().onCloseRequested(() => saveLayout(latest.current));
    return () => void unlisten.then((f) => f());
  }, [persistLayout]);

  useKeymap(layout, dispatch);

  const changeTheme = (pref: ThemePref) => {
    setTheme(pref);
    applyTheme(pref);
    commands.settingSet("theme", JSON.stringify(pref)).catch(console.error);
  };

  return (
    <div className="grid h-full grid-cols-[220px_1fr] grid-rows-[1fr_24px]">
      <Sidebar onNavigate={(view) => dispatch({ type: "navigate", view })} />
      <main className="flex min-w-0">
        {layout.panes.map((pane) => (
          <PaneView
            key={pane.id}
            pane={pane}
            focused={pane.id === layout.activePaneId}
            split={layout.panes.length > 1}
            dispatch={dispatch}
          />
        ))}
      </main>
      <Footer theme={theme} onTheme={changeTheme} />
    </div>
  );
}
