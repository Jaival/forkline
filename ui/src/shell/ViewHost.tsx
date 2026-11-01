import type { ViewRef } from "../lib/layout";
import { HomeView } from "../features/home/HomeView";
import { IssueView } from "../features/issue/IssueView";
import { Placeholder } from "../features/placeholder/Placeholder";

/** Maps a tab's view to its component. Each feature agent replaces its Placeholder. */
export function ViewHost({ view, onNavigate }: { view: ViewRef; onNavigate: (v: ViewRef) => void }) {
  switch (view.kind) {
    case "home":
      return <HomeView onNavigate={onNavigate} />;
    case "issue":
    case "pr":
      return <IssueView id={view.id} />;
    default:
      return <Placeholder view={view} />;
  }
}
