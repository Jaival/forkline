-- Phase 1 columns used by more than one agent.

-- Starred by the signed-in user. Filled by sync (agent 1), shown by the
-- Starred view (agent 2).
ALTER TABLE repos ADD COLUMN starred INTEGER NOT NULL DEFAULT 0;
CREATE INDEX repos_starred ON repos(starred, owner, name);

-- Deep sync (issues, PRs, comments, reviews, checks) runs only for pinned
-- repos. All repos the user can access get metadata only.
