-- Merge method last sent per repository (GPUI parity): the Merge button
-- offers it first. Local only; before the first merge in a repo the app
-- uses a merge commit.
CREATE TABLE repo_merge_methods (
  repo_id INTEGER PRIMARY KEY REFERENCES repos(id) ON DELETE CASCADE,
  -- `fl_types::MergeMethod::as_str`: merge, squash, rebase.
  method TEXT NOT NULL
);
