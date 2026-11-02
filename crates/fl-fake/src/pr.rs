//! Fixture data owned by agent 4 (reviews, review threads, checks, PR commits).
//! Runs after the base repos / issues / pr_details rows exist.
//! Keep it deterministic: derive every value from ids, no randomness.

use fl_db::rusqlite::{Result, Transaction, params};

const NOW: i64 = 1_798_000_000;

const REVIEWERS: [(i64, &str); 4] = [
    (9001, "pr-ana"),
    (9002, "pr-bo"),
    (9003, "pr-chen"),
    (9004, "pr-dara"),
];

const PATHS: [&str; 6] = [
    "src/lib.rs",
    "src/sync/engine.rs",
    "src/db/queries.rs",
    "ui/src/App.tsx",
    "docs/ARCHITECTURE.md",
    "Cargo.toml",
];

const MERGEABLE: [&str; 3] = ["MERGEABLE", "CONFLICTING", "UNKNOWN"];
const DECISION: [Option<&str>; 4] = [
    Some("APPROVED"),
    Some("CHANGES_REQUESTED"),
    Some("REVIEW_REQUIRED"),
    None,
];
const CHECK_STATE: [&str; 4] = ["SUCCESS", "FAILURE", "PENDING", "ERROR"];
const REVIEW_STATE: [&str; 4] = ["APPROVED", "CHANGES_REQUESTED", "COMMENTED", "DISMISSED"];
const MERGE_STATE: [&str; 4] = ["CLEAN", "DIRTY", "BLOCKED", "UNSTABLE"];

fn oid(seed: i64) -> String {
    format!("{:040x}", (seed as u128).wrapping_mul(0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C835) >> 8)
}

pub fn seed(tx: &Transaction<'_>) -> Result<()> {
    for (id, login) in REVIEWERS {
        tx.execute(
            "INSERT OR IGNORE INTO users(id, login, name) VALUES (?1, ?2, ?2)",
            params![id, login],
        )?;
    }
    let prs: Vec<(i64, i64, i64)> = tx
        .prepare("SELECT id, repo_id, number FROM issues WHERE is_pr = 1")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_>>()?;

    for (id, repo_id, number) in prs {
        let k = id as usize;
        let head = oid(id);
        let base = oid(id + 1_000_000);
        let draft = id % 11 == 0;
        tx.execute(
            "UPDATE pr_details SET base_oid = ?2, head_oid = ?3, head_ref = ?4, is_draft = ?5,
                    mergeable = ?6, merge_state = ?7, review_decision = ?8, check_state = ?9
             WHERE issue_id = ?1",
            params![
                id,
                base,
                head,
                format!("feature/change-{number}"),
                draft,
                MERGEABLE[k % MERGEABLE.len()],
                MERGE_STATE[k % MERGE_STATE.len()],
                DECISION[(k / 3) % DECISION.len()],
                CHECK_STATE[(k / 2) % CHECK_STATE.len()],
            ],
        )?;
        tx.execute(
            "INSERT INTO pr_review_requests(pr_id, reviewer_login) VALUES (?1, ?2)",
            params![id, REVIEWERS[k % 4].1],
        )?;

        // Commits: 1 to 4, oldest first; the last one is the head.
        let commit_count = 1 + k % 4;
        for pos in 0..commit_count {
            let c = if pos + 1 == commit_count { head.clone() } else { oid(id + 2_000_000 * (pos as i64 + 1)) };
            tx.execute(
                "INSERT INTO pr_commits(pr_id, oid, position) VALUES (?1, ?2, ?3)",
                params![id, c, pos as i64],
            )?;
            tx.execute(
                "INSERT OR IGNORE INTO commits(repo_id, oid, headline, author_login, authored_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    repo_id,
                    c,
                    format!("Step {} of change {number}", pos + 1),
                    REVIEWERS[(k + pos) % 4].1,
                    NOW - 40_000 + (pos as i64) * 3_600,
                ],
            )?;
        }

        // Checks on the head commit.
        for (n, name) in ["build", "test", "lint", "audit"].iter().enumerate().take(2 + k % 3) {
            let (status, conclusion) = match (k + n) % 5 {
                0 => ("IN_PROGRESS", None),
                1 => ("QUEUED", None),
                2 => ("COMPLETED", Some("FAILURE")),
                3 => ("COMPLETED", Some("SKIPPED")),
                _ => ("COMPLETED", Some("SUCCESS")),
            };
            tx.execute(
                "INSERT INTO checks(id, repo_id, commit_oid, name, status, conclusion, url, started_at, completed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id * 10 + n as i64,
                    repo_id,
                    head,
                    name,
                    status,
                    conclusion,
                    format!("https://example.invalid/runs/{id}/{name}"),
                    NOW - 30_000,
                    conclusion.map(|_| NOW - 29_000),
                ],
            )?;
        }

        // Reviews: 0 to 3.
        let mut review_ids = Vec::new();
        for n in 0..(k % 4) {
            let rid = id * 10 + n as i64;
            review_ids.push(rid);
            tx.execute(
                "INSERT INTO reviews(id, pr_id, author_id, state, body_html, submitted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    rid,
                    id,
                    REVIEWERS[(k + n) % 4].0,
                    REVIEW_STATE[(k + n) % 4],
                    "<p>Looks reasonable overall. A few notes inline.</p>",
                    NOW - 20_000 + (n as i64) * 1_000,
                ],
            )?;
        }

        // Review threads: 0 to 3, on real-looking paths and lines.
        for n in 0..(k % 4) {
            let tid = id * 10 + n as i64;
            let path = PATHS[(k + n) % PATHS.len()];
            let line = 12 + ((k * 7 + n * 13) % 60) as i64;
            let outdated = (k + n).is_multiple_of(7);
            tx.execute(
                "INSERT INTO review_threads(id, node_id, pr_id, path, side, line, start_line, original_line, commit_oid, is_resolved, is_outdated)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    tid,
                    format!("PRRT_{tid}"),
                    id,
                    path,
                    if (k + n).is_multiple_of(5) { "LEFT" } else { "RIGHT" },
                    (!outdated).then_some(line),
                    (n == 1).then_some(line - 2),
                    line,
                    head,
                    (k + n).is_multiple_of(3),
                    outdated,
                ],
            )?;
            for c in 0..(1 + (k + n) % 3) {
                tx.execute(
                    "INSERT INTO review_comments(id, thread_id, review_id, author_id, body_html, body_md, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        tid * 10 + c as i64,
                        tid,
                        review_ids.get(n).copied(),
                        REVIEWERS[(k + n + c) % 4].0,
                        format!("<p>Comment {} on <code>{path}</code>: can this handle the empty case?</p>", c + 1),
                        format!("Comment {} on `{path}`: can this handle the empty case?", c + 1),
                        NOW - 19_000 + (c as i64) * 500,
                    ],
                )?;
            }
        }

        // Conversation comments: 0 to 2. Ids sit far above the other seeds.
        for n in 0..(k % 3) {
            tx.execute(
                "INSERT INTO comments(id, issue_id, author_id, body_html, body_md, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![
                    50_000_000 + id * 10 + n as i64,
                    id,
                    REVIEWERS[(k + n) % 4].0,
                    "<p>Thanks, I will <strong>update</strong> this today.</p>",
                    "Thanks, I will **update** this today.",
                    NOW - 15_000 + (n as i64) * 700,
                ],
            )?;
        }
    }
    Ok(())
}
