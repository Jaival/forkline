-- G3 step 1: lists across all repos, auto-pin, sync scope.

-- Items the viewer's GitHub searches returned on the last search sync.
-- `query` is one of 'authored', 'review_requested', 'assigned'. Each search
-- sync replaces the rows of its query, so an item that leaves a search
-- leaves the list. The items themselves are normal `issues` rows.
CREATE TABLE search_hits (
  query     TEXT NOT NULL,
  issue_id  INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  PRIMARY KEY (query, issue_id)
) WITHOUT ROWID;
CREATE INDEX search_hits_issue ON search_hits(issue_id);

-- 1 = the repo is in `/user/repos` (owner, collaborator, organization member).
-- Starred-only and Explore-only repos stay 0. Auto-pin picks from these.
ALTER TABLE repos ADD COLUMN member INTEGER NOT NULL DEFAULT 0;
-- `open_issues_count` from REST (open issues + open PRs). NULL = unknown.
ALTER TABLE repos ADD COLUMN open_issues INTEGER;
-- Deep sync scope of a pinned repo: 'all' = every issue and PR; 'mine' =
-- only items that involve the viewer (author, assignee, mention, comment,
-- review request). NULL = automatic: 'mine' for large repos (see
-- `fl_sync::sync::LARGE_REPO_OPEN`). Local only; set by the user.
ALTER TABLE repos ADD COLUMN sync_scope TEXT CHECK (sync_scope IN ('all', 'mine'));

-- Re-read `/user/repos` without the ETag so `member` and `open_issues` fill.
DELETE FROM sync_state WHERE key IN ('repos', 'starred');
