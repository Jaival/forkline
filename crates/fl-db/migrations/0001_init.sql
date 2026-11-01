-- Forkline schema v1. Timestamps are unix seconds (UTC). Booleans are 0/1.
-- IDs are GitHub databaseIds unless noted.

CREATE TABLE repos (
  id              INTEGER PRIMARY KEY,
  node_id         TEXT NOT NULL,
  owner           TEXT NOT NULL,
  name            TEXT NOT NULL,
  description     TEXT,
  language        TEXT,
  default_branch  TEXT NOT NULL DEFAULT 'main',
  license         TEXT,
  is_private      INTEGER NOT NULL DEFAULT 0,
  is_fork         INTEGER NOT NULL DEFAULT 0,
  stars           INTEGER NOT NULL DEFAULT 0,
  forks           INTEGER NOT NULL DEFAULT 0,
  pushed_at       INTEGER,
  updated_at      INTEGER NOT NULL,
  languages_json  TEXT,                       -- [{"name":"Rust","bytes":123}]
  pinned          INTEGER NOT NULL DEFAULT 0,
  sync_enabled    INTEGER NOT NULL DEFAULT 1,
  clone_path      TEXT,                       -- internal blobless clone
  UNIQUE (owner, name)
);
CREATE INDEX repos_pinned ON repos(pinned, owner, name);

CREATE TABLE users (
  id          INTEGER PRIMARY KEY,
  login       TEXT NOT NULL UNIQUE,
  name        TEXT,
  avatar_url  TEXT
);

CREATE TABLE labels (
  id           INTEGER PRIMARY KEY,
  repo_id      INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  color        TEXT NOT NULL,
  description  TEXT
);
CREATE INDEX labels_repo ON labels(repo_id, name);

-- Issues and pull requests (is_pr = 1).
CREATE TABLE issues (
  id              INTEGER PRIMARY KEY,
  node_id         TEXT NOT NULL,
  repo_id         INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  number          INTEGER NOT NULL,
  is_pr           INTEGER NOT NULL,
  state           TEXT NOT NULL CHECK (state IN ('open','closed','merged')),
  title           TEXT NOT NULL,
  body_html       TEXT,
  body_md         TEXT,
  author_id       INTEGER REFERENCES users(id),
  milestone       TEXT,
  comments_count  INTEGER NOT NULL DEFAULT 0,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  closed_at       INTEGER,
  last_viewed_at  INTEGER,                    -- local only; unread = updated_at > last_viewed_at
  UNIQUE (repo_id, number)
);
CREATE INDEX issues_repo_list   ON issues(repo_id, is_pr, state, updated_at DESC);
CREATE INDEX issues_author_list ON issues(author_id, state, updated_at DESC);
CREATE INDEX issues_global_list ON issues(is_pr, state, updated_at DESC);
CREATE INDEX issues_updated     ON issues(updated_at DESC);

CREATE TABLE issue_labels (
  issue_id  INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  label_id  INTEGER NOT NULL REFERENCES labels(id) ON DELETE CASCADE,
  PRIMARY KEY (issue_id, label_id)
) WITHOUT ROWID;
CREATE INDEX issue_labels_label ON issue_labels(label_id, issue_id);

CREATE TABLE issue_assignees (
  issue_id  INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  user_id   INTEGER NOT NULL REFERENCES users(id),
  PRIMARY KEY (issue_id, user_id)
) WITHOUT ROWID;
CREATE INDEX issue_assignees_user ON issue_assignees(user_id, issue_id);

-- "Closes #n" links and linked PR chips.
CREATE TABLE issue_links (
  issue_id  INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  pr_id     INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  PRIMARY KEY (issue_id, pr_id)
) WITHOUT ROWID;
CREATE INDEX issue_links_pr ON issue_links(pr_id, issue_id);

CREATE TABLE pr_details (
  issue_id         INTEGER PRIMARY KEY REFERENCES issues(id) ON DELETE CASCADE,
  base_ref         TEXT NOT NULL,
  head_ref         TEXT NOT NULL,
  base_oid         TEXT NOT NULL,
  head_oid         TEXT NOT NULL,
  is_draft         INTEGER NOT NULL DEFAULT 0,
  mergeable        TEXT,                      -- MERGEABLE | CONFLICTING | UNKNOWN
  merge_state      TEXT,                      -- GitHub mergeStateStatus
  review_decision  TEXT,                      -- APPROVED | CHANGES_REQUESTED | REVIEW_REQUIRED
  check_state      TEXT,                      -- SUCCESS | FAILURE | PENDING | ERROR
  additions        INTEGER NOT NULL DEFAULT 0,
  deletions        INTEGER NOT NULL DEFAULT 0,
  changed_files    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE pr_review_requests (
  pr_id           INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  reviewer_login  TEXT NOT NULL,             -- user login or org/team slug
  PRIMARY KEY (pr_id, reviewer_login)
) WITHOUT ROWID;
CREATE INDEX pr_review_requests_login ON pr_review_requests(reviewer_login, pr_id);

CREATE TABLE pr_commits (
  pr_id     INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  oid       TEXT NOT NULL,
  position  INTEGER NOT NULL,
  PRIMARY KEY (pr_id, oid)
) WITHOUT ROWID;

CREATE TABLE pr_viewed_files (
  pr_id     INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  path      TEXT NOT NULL,
  head_oid  TEXT NOT NULL,                    -- viewed state resets when head changes
  PRIMARY KEY (pr_id, path)
) WITHOUT ROWID;

CREATE TABLE comments (
  id          INTEGER PRIMARY KEY,            -- negative = local optimistic comment
  issue_id    INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  author_id   INTEGER REFERENCES users(id),
  body_html   TEXT,
  body_md     TEXT,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);
CREATE INDEX comments_issue ON comments(issue_id, created_at);

CREATE TABLE timeline_events (
  id            INTEGER PRIMARY KEY,
  issue_id      INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  kind          TEXT NOT NULL,               -- labeled, closed, reopened, referenced, ...
  actor_id      INTEGER REFERENCES users(id),
  created_at    INTEGER NOT NULL,
  payload_json  TEXT
);
CREATE INDEX timeline_issue ON timeline_events(issue_id, created_at);

CREATE TABLE reviews (
  id            INTEGER PRIMARY KEY,
  pr_id         INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  author_id     INTEGER REFERENCES users(id),
  state         TEXT NOT NULL,               -- APPROVED | CHANGES_REQUESTED | COMMENTED | PENDING | DISMISSED
  body_html     TEXT,
  submitted_at  INTEGER
);
CREATE INDEX reviews_pr ON reviews(pr_id, submitted_at);

CREATE TABLE review_threads (
  id             INTEGER PRIMARY KEY,
  node_id        TEXT NOT NULL,
  pr_id          INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  path           TEXT NOT NULL,
  side           TEXT NOT NULL CHECK (side IN ('LEFT','RIGHT')),
  line           INTEGER,
  start_line     INTEGER,
  original_line  INTEGER,
  commit_oid     TEXT,
  is_resolved    INTEGER NOT NULL DEFAULT 0,
  is_outdated    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX review_threads_pr ON review_threads(pr_id, path);

CREATE TABLE review_comments (
  id          INTEGER PRIMARY KEY,
  thread_id   INTEGER NOT NULL REFERENCES review_threads(id) ON DELETE CASCADE,
  review_id   INTEGER REFERENCES reviews(id) ON DELETE SET NULL,
  author_id   INTEGER REFERENCES users(id),
  body_html   TEXT,
  body_md     TEXT,
  created_at  INTEGER NOT NULL
);
CREATE INDEX review_comments_thread ON review_comments(thread_id, created_at);

CREATE TABLE commits (
  repo_id            INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  oid                TEXT NOT NULL,
  headline           TEXT NOT NULL,
  author_login       TEXT,
  authored_at        INTEGER NOT NULL,
  check_state        TEXT,
  on_default_branch  INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (repo_id, oid)
) WITHOUT ROWID;
CREATE INDEX commits_default ON commits(repo_id, on_default_branch, authored_at DESC);

CREATE TABLE checks (
  id            INTEGER PRIMARY KEY,
  repo_id       INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  commit_oid    TEXT NOT NULL,
  name          TEXT NOT NULL,
  status        TEXT NOT NULL,               -- QUEUED | IN_PROGRESS | COMPLETED
  conclusion    TEXT,                        -- SUCCESS | FAILURE | NEUTRAL | CANCELLED | SKIPPED | TIMED_OUT
  url           TEXT,
  started_at    INTEGER,
  completed_at  INTEGER
);
CREATE INDEX checks_commit ON checks(repo_id, commit_oid);

CREATE TABLE releases (
  id            INTEGER PRIMARY KEY,
  repo_id       INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  tag           TEXT NOT NULL,
  name          TEXT,
  published_at  INTEGER
);
CREATE INDEX releases_repo ON releases(repo_id, published_at DESC);

CREATE TABLE repo_contributors (
  repo_id        INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  user_id        INTEGER NOT NULL REFERENCES users(id),
  contributions  INTEGER NOT NULL,
  PRIMARY KEY (repo_id, user_id)
) WITHOUT ROWID;

CREATE TABLE star_history (
  repo_id  INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  day      INTEGER NOT NULL,                 -- unix day number (seconds / 86400)
  stars    INTEGER NOT NULL,
  PRIMARY KEY (repo_id, day)
) WITHOUT ROWID;

CREATE TABLE notifications (
  id            TEXT PRIMARY KEY,            -- GitHub thread id
  repo_id       INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
  subject_type  TEXT NOT NULL,               -- PullRequest | Issue | Release | ...
  issue_id      INTEGER REFERENCES issues(id) ON DELETE SET NULL,
  title         TEXT NOT NULL,
  reason        TEXT NOT NULL,               -- review_requested | mention | author | ...
  unread        INTEGER NOT NULL DEFAULT 1,
  done          INTEGER NOT NULL DEFAULT 0,
  updated_at    INTEGER NOT NULL
);
CREATE INDEX notifications_inbox ON notifications(done, repo_id, updated_at DESC);

CREATE TABLE action_queue (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  kind             TEXT NOT NULL,            -- comment | label | assign | close | reopen | approve | merge | ...
  payload_json     TEXT NOT NULL,
  target_table     TEXT NOT NULL,
  target_id        INTEGER NOT NULL,
  state            TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','failed')),
  attempts         INTEGER NOT NULL DEFAULT 0,
  next_attempt_at  INTEGER NOT NULL,
  last_error       TEXT,
  created_at       INTEGER NOT NULL
);
CREATE INDEX action_queue_due ON action_queue(state, next_attempt_at);

-- Fields changed optimistically. Sync must not overwrite them; on final
-- failure the old value is restored.
CREATE TABLE pending_fields (
  table_name      TEXT NOT NULL,
  row_id          INTEGER NOT NULL,
  field           TEXT NOT NULL,
  action_id       INTEGER NOT NULL REFERENCES action_queue(id) ON DELETE CASCADE,
  old_value_json  TEXT,
  PRIMARY KEY (table_name, row_id, field)
) WITHOUT ROWID;
CREATE INDEX pending_fields_action ON pending_fields(action_id);

CREATE TABLE sync_state (
  key             TEXT PRIMARY KEY,          -- e.g. 'notifications', 'repo:123:issues'
  etag            TEXT,
  last_modified   TEXT,
  cursor          TEXT,
  last_synced_at  INTEGER,
  poll_interval   INTEGER
) WITHOUT ROWID;

CREATE TABLE settings (
  key         TEXT PRIMARY KEY,
  value_json  TEXT NOT NULL
) WITHOUT ROWID;

-- Tabs and panes, owned by the UI (opaque JSON).
CREATE TABLE ui_layout (
  id    INTEGER PRIMARY KEY CHECK (id = 1),
  json  TEXT NOT NULL
);

-- One row per issue (kind = 'issue', title + body) and one per comment
-- (kind = 'comment', body only). Search groups results by issue_id.
CREATE VIRTUAL TABLE search_fts USING fts5(
  title,
  body,
  issue_id UNINDEXED,
  kind UNINDEXED,
  tokenize = 'unicode61 remove_diacritics 2',
  prefix = '2 3'
);
