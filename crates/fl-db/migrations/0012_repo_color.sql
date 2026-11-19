-- Repository colour picked by the user (G2 step 3): an index into the
-- theme's set of 10 repo hues. NULL = from a hash of `owner/name`.
-- Local only; sync never writes this column.
ALTER TABLE repos ADD COLUMN color INTEGER CHECK (color BETWEEN 0 AND 9);
