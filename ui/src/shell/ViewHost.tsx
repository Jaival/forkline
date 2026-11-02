import type { ViewRef } from "../lib/layout";
import { HomeView } from "../features/home/HomeView";
import { InboxView } from "../features/inbox/InboxView";
import { IssueView } from "../features/issue/IssueView";
import { ListView } from "../features/lists/ListView";
import { PrView } from "../features/pr/PrView";
import { Placeholder } from "../features/placeholder/Placeholder";
import { RepoOverviewView } from "../features/repo/RepoOverviewView";
import { ReposView } from "../features/repo/ReposView";

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
      return <IssueView id={view.id} onNavigate={onNavigate} />;
    case "pr":
      return <PrView id={view.id} />;
    case "repo":
      return <RepoOverviewView repoId={view.repoId} onNavigate={onNavigate} />;
    case "repos":
      return <ReposView starredOnly={false} onNavigate={onNavigate} />;
    case "starred":
      return <ReposView starredOnly onNavigate={onNavigate} />;
    default:
      return <Placeholder view={view} />;
  }
}
