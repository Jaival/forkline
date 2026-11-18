-- Label filter in issue lists looks labels up by name across repos.
CREATE INDEX labels_name ON labels(name);
