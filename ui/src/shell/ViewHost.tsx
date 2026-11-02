import type { ViewRef } from "../lib/layout";
import { HomeView } from "../features/home/HomeView";
import { InboxView } from "../features/inbox/InboxView";
import { IssueView } from "../features/issue/IssueView";
import { ListView } from "../features/lists/ListView";
import { Placeholder } from "../features/placeholder/Placeholder";

/** Maps a tab's view to its component. Each feature agent replaces its Placeholder. */
export function ViewHost({ view, onNavigate }: { view: ViewRef; onNavigate: (v: ViewRef) => void }) {
  switch (view.kind) {
    case "home":
      return <HomeView onNavigate={onNavigate} />;
    case "inbox":
      return <InboxView onNavigate={onNavigate} />;
    case "pulls":
    case "issues":
    case "reviews":
      return <ListView view={view} onNavigate={onNavigate} />;
    case "issue":
    case "pr":
      return <IssueView id={view.id} onNavigate={onNavigate} />;
    default:
      return <Placeholder view={view} />;
  }
}
