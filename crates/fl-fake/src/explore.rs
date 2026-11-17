//! Explore: 20 followed users and about 500 recent stars, on 40 repos known
//! only from Explore plus some of the user's own repos.

use fl_db::rusqlite::{self, Transaction, params};

const FOLLOWED: i64 = 20;
const STARS_EACH: i64 = 25;
const EXPLORE_REPOS: i64 = 40;
const FIRST_USER: i64 = 100;
const FIRST_REPO: i64 = 1_001;
const LANGS: [&str; 5] = ["Rust", "TypeScript", "Go", "Python", "Zig"];

pub fn seed(tx: &Transaction<'_>, now: i64) -> rusqlite::Result<()> {
    for k in 0..EXPLORE_REPOS {
        let id = FIRST_REPO + k;
        tx.execute(
            "INSERT INTO repos(id,node_id,owner,name,description,language,default_branch,stars,forks,updated_at,sync_enabled,explore_only)
             VALUES (?1,?2,?3,?4,?5,?6,'main',?7,?8,?9,0,1)",
            params![
                id,
                format!("R_{id}"),
                format!("org-{}", k % 8),
                format!("tool-{k:02}"),
                format!("A useful tool number {k}"),
                LANGS[(k % 5) as usize],
                500 + k * 97,
                k * 7,
                now - k * 3_600,
            ],
        )?;
    }
    for u in 0..FOLLOWED {
        let user = FIRST_USER + u;
        tx.execute(
            "INSERT OR IGNORE INTO users(id, login, name) VALUES (?1, ?2, ?2)",
            params![user, format!("dev-{u:02}")],
        )?;
        tx.execute("INSERT INTO following(user_id) VALUES (?1)", [user])?;
        for s in 0..STARS_EACH {
            // Mostly explore-only repos; every fifth star is one of the user's repos.
            let repo = if s % 5 == 4 { 1 + (u + s) % 50 } else { FIRST_REPO + (u * 7 + s) % EXPLORE_REPOS };
            tx.execute(
                "INSERT OR IGNORE INTO explore_stars(user_id, repo_id, starred_at) VALUES (?1, ?2, ?3)",
                params![user, repo, now - (u * STARS_EACH + s) * 1_800],
            )?;
        }
    }
    Ok(())
}
