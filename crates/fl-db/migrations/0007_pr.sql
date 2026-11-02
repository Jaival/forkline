-- PR view: ordered commit list and checks list.
CREATE INDEX pr_commits_pos ON pr_commits(pr_id, position);
CREATE INDEX checks_commit_name ON checks(repo_id, commit_oid, name);
