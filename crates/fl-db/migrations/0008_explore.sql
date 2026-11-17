-- Explore: repos starred by the people the signed-in user follows.

-- People the signed-in user follows (REST /user/following, ETag).
CREATE TABLE following (
  user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE
);

-- Recent stars of followed users (REST /users/{login}/starred with the
-- star+json media type, which gives starred_at). Unfollowing cascades.
CREATE TABLE explore_stars (
  user_id    INTEGER NOT NULL REFERENCES following(user_id) ON DELETE CASCADE,
  repo_id    INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  starred_at INTEGER NOT NULL,
  PRIMARY KEY (user_id, repo_id)
) WITHOUT ROWID;
CREATE INDEX explore_stars_recent ON explore_stars(starred_at DESC);
CREATE INDEX explore_stars_repo   ON explore_stars(repo_id, starred_at);

-- 1 = the repo is known only from Explore; the Repositories view hides it.
-- Sync sets 0 when the repo appears in the user's own or starred lists, and
-- deletes explore-only repos that no explore_stars row references.
ALTER TABLE repos ADD COLUMN explore_only INTEGER NOT NULL DEFAULT 0;
