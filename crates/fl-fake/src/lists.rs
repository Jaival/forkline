//! Fixture data owned by agent 3 (labels, assignees, comments, timeline, notifications).
//! Runs after the base repos / issues / pr_details rows exist.
//! Keep it deterministic: derive every value from ids, no randomness.

use fl_db::rusqlite::{Result, Transaction, params};

const USERS: [&str; 6] = ["octocat", "hubot", "monalisa", "defunkt", "mojombo", "pjhyett"];
const LABELS: [(&str, &str); 6] = [
    ("bug", "d73a4a"),
    ("enhancement", "a2eeef"),
    ("documentation", "0075ca"),
    ("good first issue", "7057ff"),
    ("help wanted", "008672"),
    ("performance", "fbca04"),
];

const BODY_TEMPLATES: [&str; 4] = [
    "## Summary\n\nSomething is off in this area. Steps to reproduce:\n\n1. Open the project\n2. Run `cargo test`\n3. Watch it fail\n\n```rust\nfn main() {\n    let answer = 6 * 7;\n    assert_eq!(answer, 42);\n}\n```\n\n- [x] Reproduced locally\n- [ ] Add a regression test\n- [ ] Update the docs\n\nSee [the guide](https://example.com/guide) for details.",
    "Proposal: make this configurable.\n\n> It should work out of the box, but power users want control.\n\n**Tasks**\n\n- [x] Design the setting\n- [x] Write the parser\n- [ ] Wire it to the UI\n\nInline `code`, **bold**, _italic_ and ~~strikethrough~~ all render.",
    "Short description with a table:\n\n| Case | Before | After |\n|------|--------|-------|\n| cold | 120 ms | 40 ms |\n| warm | 30 ms  | 9 ms  |\n\n```sh\nbun run bench\n```",
    "Plain text body. Nothing special here, but it is long enough to wrap onto more than one line when the window is narrow, which is what we want to check.",
];

const COMMENT_TEMPLATES: [&str; 5] = [
    "Thanks for the report! I can reproduce this.\n\n```\nerror: assertion failed\n```",
    "Working on a fix.\n\n- [x] Find the cause\n- [ ] Write the patch",
    "+1, this also affects me on Windows 11.",
    "Could you share the output of `forkline --version`?\n\n> Version 0.1.0 on Windows 11",
    "Fixed in the linked PR. Closing once it is merged. :tada:",
];

const EVENT_KINDS: [&str; 4] = ["labeled", "assigned", "referenced", "closed"];

pub fn seed(tx: &Transaction<'_>) -> Result<()> {
    // `viewer` is what agent 1's sign-in writes; used by "Review requests".
    tx.execute(
        "INSERT OR IGNORE INTO settings(key, value_json) VALUES ('viewer', '{\"id\":1,\"login\":\"octocat\"}')",
        [],
    )?;
    for (i, login) in USERS.iter().enumerate().skip(1) {
        tx.execute(
            "INSERT OR IGNORE INTO users(id, login, name) VALUES (?1, ?2, ?2)",
            params![i as i64 + 1, login],
        )?;
    }

    // Labels (ids repo*10 + k), then a deterministic spread over issues.
    tx.execute_batch(
        "UPDATE users SET name = 'Mona' WHERE id = 1;
         -- Authors rotate over the six users; every 7th item is closed, every 11th PR merged.
         UPDATE issues SET author_id = 1 + (id % 6),
             state = CASE WHEN is_pr = 1 AND id % 11 = 0 THEN 'merged'
                          WHEN id % 7 = 0 THEN 'closed' ELSE 'open' END,
             closed_at = CASE WHEN id % 7 = 0 OR (is_pr = 1 AND id % 11 = 0) THEN updated_at ELSE NULL END;",
    )?;
    for (k, (name, color)) in LABELS.iter().enumerate() {
        tx.execute(
            "INSERT INTO labels(id, repo_id, name, color) SELECT id * 10 + ?1, id, ?2, ?3 FROM repos",
            params![k as i64 + 1, name, color],
        )?;
    }
    // Each issue gets 0-2 labels from its repo, 0-2 assignees.
    tx.execute_batch(
        "INSERT INTO issue_labels(issue_id, label_id)
           SELECT id, repo_id * 10 + 1 + (id % 6) FROM issues WHERE id % 3 <> 2;
         INSERT OR IGNORE INTO issue_labels(issue_id, label_id)
           SELECT id, repo_id * 10 + 1 + ((id / 6) % 6) FROM issues WHERE id % 4 = 0;
         INSERT INTO issue_assignees(issue_id, user_id)
           SELECT id, 1 + (id % 5) FROM issues WHERE id % 2 = 0;
         INSERT OR IGNORE INTO issue_assignees(issue_id, user_id)
           SELECT id, 1 + ((id / 5) % 6) FROM issues WHERE id % 9 = 0;
         -- Octocat is asked to review a third of the open PRs.
         INSERT INTO pr_review_requests(pr_id, reviewer_login)
           SELECT id, 'octocat' FROM issues WHERE is_pr = 1 AND state = 'open' AND id % 2 = 1;
         INSERT OR IGNORE INTO pr_review_requests(pr_id, reviewer_login)
           SELECT id, 'hubot' FROM issues WHERE is_pr = 1 AND state = 'open' AND id % 5 = 0;
         -- Some issues are fixed by the PR that follows them in the same repo.
         INSERT INTO issue_links(issue_id, pr_id)
           SELECT a.id, b.id FROM issues a JOIN issues b ON b.repo_id = a.repo_id AND b.number = a.number + 2
           WHERE a.is_pr = 0 AND b.is_pr = 1 AND a.number % 4 = 1;",
    )?;

    // Richer bodies, comments (0-9 each, about 90k), timeline events for every item.
    let mut body = tx.prepare("UPDATE issues SET body_md = ?2 WHERE id = ?1")?;
    let mut comment = tx.prepare(
        "INSERT INTO comments(id, issue_id, author_id, body_md, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?5)",
    )?;
    let mut event = tx.prepare(
        "INSERT INTO timeline_events(id, issue_id, kind, actor_id, created_at, payload_json) VALUES (?1,?2,?3,?4,?5,?6)",
    )?;
    let mut comment_id = 0_i64;
    let mut event_id = 0_i64;
    let mut stmt = tx.prepare("SELECT id, created_at, updated_at FROM issues")?;
    let items = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))?
        .collect::<Result<Vec<_>>>()?;
    for (id, created, updated) in items {
        body.execute(params![id, BODY_TEMPLATES[(id % 4) as usize]])?;
        let n_comments = id % 10;
        for c in 0..n_comments {
            comment_id += 1;
            let at = created + (c + 1) * (updated - created) / (n_comments + 1);
            comment.execute(params![
                comment_id,
                id,
                1 + (id + c) % 6,
                COMMENT_TEMPLATES[((id + c) % 5) as usize],
                at
            ])?;
        }
        tx.execute("UPDATE issues SET comments_count = ?2 WHERE id = ?1", params![id, n_comments])?;
        for e in 0..(id % 3) {
            event_id += 1;
            let kind = EVENT_KINDS[((id + e) % 4) as usize];
            let payload = match kind {
                "labeled" => Some(format!("{{\"label\":\"{}\"}}", LABELS[(id % 6) as usize].0)),
                "assigned" => Some(format!("{{\"assignee\":\"{}\"}}", USERS[(id % 5) as usize])),
                "referenced" => Some(format!("{{\"ref\":\"#{}\"}}", id % 90 + 1)),
                _ => None,
            };
            event.execute(params![event_id, id, kind, 1 + (id + e) % 6, created + (e + 1) * 600, payload])?;
        }
    }
    drop((body, comment, event, stmt));

    // Notifications: every 25th item, a few are done and a third are read.
    tx.execute_batch(
        "INSERT INTO notifications(id, repo_id, subject_type, issue_id, title, reason, unread, done, updated_at)
         SELECT 'n' || id, repo_id, CASE WHEN is_pr = 1 THEN 'PullRequest' ELSE 'Issue' END, id, title,
                CASE (id / 25) % 5 WHEN 0 THEN 'review_requested' WHEN 1 THEN 'mention' WHEN 2 THEN 'author'
                            WHEN 3 THEN 'assign' ELSE 'subscribed' END,
                id % 3 <> 0, id % 50 = 0, updated_at
         FROM issues WHERE id % 25 = 0;
         -- Notified items were last opened halfway through their comments,
         -- so the reading pane shows its \"New since your last view\" divider.
         UPDATE issues SET last_viewed_at = (created_at + updated_at) / 2 WHERE id % 25 = 0;
         -- A release notification without a linked issue.
         INSERT INTO notifications(id, repo_id, subject_type, issue_id, title, reason, unread, done, updated_at)
           SELECT 'rel' || id, id, 'Release', NULL, 'v1.' || id || '.0 released', 'subscribed', 1, 0, 1798000000 - id
           FROM repos WHERE id <= 8;",
    )?;
    Ok(())
}
