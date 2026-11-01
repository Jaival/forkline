import { useEffect, useReducer, useState } from "react";
import { commands } from "../bindings";
import { type Layout, reduceLayout } from "../lib/layout";
import { applyTheme, type ThemePref } from "../lib/theme";
import { driver } from "./driver";
import { Footer } from "./Footer";
import { useKeymap } from "./keymap";
import { PaneView } from "./PaneView";
import { Sidebar } from "./Sidebar";

export function Shell({ initialLayout, initialTheme }: { initialLayout: Layout; initialTheme: ThemePref }) {
  const [layout, dispatch] = useReducer(reduceLayout, initialLayout);
  const [theme, setTheme] = useState(initialTheme);

  useEffect(() => {
    driver.dispatch = dispatch;
    driver.layout = layout;
  }, [dispatch, layout]);

  // Save tabs + panes shortly after each change.
  useEffect(() => {
    const t = setTimeout(() => void commands.layoutSave(JSON.stringify(layout)), 300);
    return () => clearTimeout(t);
  }, [layout]);

  useKeymap(layout, dispatch);

  const changeTheme = (pref: ThemePref) => {
    setTheme(pref);
    applyTheme(pref);
    void commands.settingSet("theme", JSON.stringify(pref));
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
