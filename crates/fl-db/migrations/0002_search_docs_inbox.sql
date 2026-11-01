-- Search rows get a stable rowid owned by `search_docs`, so one issue's rows
-- can be found and replaced through an index. Deleting an issue (or its repo)
-- cascades to `search_docs`, and the trigger removes the matching FTS row.

DROP TABLE search_fts;

CREATE TABLE search_docs (
  id          INTEGER PRIMARY KEY,           -- = search_fts rowid
  issue_id    INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
  comment_id  INTEGER NOT NULL DEFAULT 0,    -- 0 = the issue's title + body
  UNIQUE (issue_id, comment_id)
);

-- Issue rows: title + body. Comment rows: body only. Search joins
-- `search_docs` on rowid and groups results by issue_id.
CREATE VIRTUAL TABLE search_fts USING fts5(
  title,
  body,
  tokenize = 'unicode61 remove_diacritics 2',
  prefix = '2 3'
);

CREATE TRIGGER search_docs_delete AFTER DELETE ON search_docs BEGIN
  DELETE FROM search_fts WHERE rowid = old.id;
END;

-- Inbox across all repos, newest first, without a sort step.
CREATE INDEX notifications_done ON notifications(done, updated_at DESC);
