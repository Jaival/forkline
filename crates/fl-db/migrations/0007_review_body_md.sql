-- Review bodies keep GitHub's markdown source next to the rendered HTML.
ALTER TABLE reviews ADD COLUMN body_md TEXT;
