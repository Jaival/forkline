//! Fixture data owned by agent 2 (languages, contributors, releases, commits, star history, starred).
//! Runs after the base repos / issues / pr_details rows exist.
//! Keep it deterministic: derive every value from ids, no randomness.

use fl_db::rusqlite::{Result, Transaction};

const NOW: i64 = 1_798_000_000;
const LOGINS: [&str; 6] = ["octocat", "hubot", "monalisa", "ferris", "gopher", "tux"];
const LANGS: [&str; 5] = ["Rust", "TypeScript", "Go", "Python", "Shell"];
const LICENSES: [&str; 3] = ["MIT", "Apache-2.0", "GPL-3.0"];
const CHECKS: [Option<&str>; 4] = [Some("SUCCESS"), Some("FAILURE"), Some("PENDING"), None];

pub fn seed(tx: &Transaction<'_>) -> Result<()> {
    for (i, login) in LOGINS.iter().enumerate().skip(1) {
        tx.execute(
            "INSERT OR IGNORE INTO users(id,login,name) VALUES (?1,?2,?2)",
            (i as i64 + 1, login),
        )?;
    }
    let repo_ids: Vec<i64> = tx
        .prepare("SELECT id FROM repos ORDER BY id")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_>>()?;
    for id in repo_ids {
        // Every 4th repo is starred; pinned repos 1..=8 include some of them.
        tx.execute(
            "UPDATE repos SET starred = ?2, license = ?3, languages_json = ?4 WHERE id = ?1",
            (
                id,
                i64::from(id % 4 == 0 || id == 2),
                LICENSES[id as usize % LICENSES.len()],
                languages_json(id),
            ),
        )?;
        for n in 0..12_i64 {
            tx.execute(
                "INSERT INTO commits(repo_id,oid,headline,author_login,authored_at,check_state,on_default_branch)
                 VALUES (?1,?2,?3,?4,?5,?6,1)",
                (
                    id,
                    format!("{id:03x}{n:04x}{:033x}", id * 7919 + n),
                    format!("Fixture commit {n} for project {id:03}"),
                    LOGINS[(id + n) as usize % LOGINS.len()],
                    NOW - n * 5_400 - id * 60,
                    CHECKS[(id + n) as usize % CHECKS.len()],
                ),
            )?;
        }
        for n in 0..(1 + id % 6) {
            tx.execute(
                "INSERT INTO releases(id,repo_id,tag,name,published_at) VALUES (?1,?2,?3,?4,?5)",
                (id * 10 + n, id, format!("v1.{n}.0"), format!("Release 1.{n}"), NOW - (6 - n) * 864_000),
            )?;
        }
        for (u, _) in LOGINS.iter().enumerate() {
            tx.execute(
                "INSERT INTO repo_contributors(repo_id,user_id,contributions) VALUES (?1,?2,?3)",
                (id, u as i64 + 1, 400 / (u as i64 + 1) + id % 17),
            )?;
        }
        // Rising curve over the last 60 days ending at the repo's star count.
        let stars = id * 13;
        let today = NOW / 86_400;
        for d in 0..60_i64 {
            tx.execute(
                "INSERT INTO star_history(repo_id,day,stars) VALUES (?1,?2,?3)",
                (id, today - 59 + d, stars * (40 + d) / 99),
            )?;
        }
    }
    Ok(())
}

fn languages_json(id: i64) -> String {
    let a = LANGS[id as usize % LANGS.len()];
    let b = LANGS[(id as usize + 1) % LANGS.len()];
    let c = LANGS[(id as usize + 3) % LANGS.len()];
    serde_json::json!([
        {"name": a, "bytes": 60_000 + id * 100},
        {"name": b, "bytes": 25_000 + id * 10},
        {"name": c, "bytes": 5_000}
    ])
    .to_string()
}
