import type { ViewRef } from "../lib/layout";
import { HomeView } from "../features/home/HomeView";
import { IssueView } from "../features/issue/IssueView";
import { Placeholder } from "../features/placeholder/Placeholder";
import { RepoOverviewView } from "../features/repo/RepoOverviewView";
import { ReposView } from "../features/repo/ReposView";

/** Maps a tab's view to its component. Each feature agent replaces its Placeholder. */
export function ViewHost({ view, onNavigate }: { view: ViewRef; onNavigate: (v: ViewRef) => void }) {
  switch (view.kind) {
    case "home":
      return <HomeView onNavigate={onNavigate} />;
    case "issue":
    case "pr":
      return <IssueView id={view.id} />;
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
