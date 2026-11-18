-- Saved queue filters (GPUI G1 step 6): a named `/` query for one queue kind.
-- Local only; GitHub has no equivalent.
CREATE TABLE saved_views (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  -- `fl_types::QueueKind::as_str`: inbox, pulls, issues, reviews.
  kind TEXT NOT NULL,
  query TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  UNIQUE (kind, name)
);
