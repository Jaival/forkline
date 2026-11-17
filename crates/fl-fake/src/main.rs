mod explore;
mod git;
mod lists;
mod pr;
mod repo;

use std::path::PathBuf;

use anyhow::{Context, bail};
use fl_db::{Db, db_path};

// 50 x 400 = 20,000 issues and PRs (ADR 0001).
const REPO_COUNT: i64 = 50;
const ISSUES_PER_REPO: i64 = 400;
const NOW: i64 = 1_798_000_000;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut data_dir = PathBuf::from("fake-data");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data-dir" => data_dir = args.next().context("--data-dir needs a path")?.into(),
            other => bail!("unknown argument: {other}"),
        }
    }
    std::fs::create_dir_all(&data_dir)?;
    let fixture = git::build(&data_dir).context("build the fixture git repository")?;
    let db = Db::open(&db_path(&data_dir)).context("open fake database")?;
    db.write(|conn| {
        let tx = conn.transaction()?;
        // Cascades to issues and search_docs; the search_docs trigger clears search_fts.
        tx.execute("DELETE FROM repos", [])?;
        tx.execute("DELETE FROM following", [])?;
        tx.execute("INSERT OR IGNORE INTO users(id,login,name) VALUES (1,'octocat','Mona')", [])?;
        for repo_id in 1..=REPO_COUNT {
            tx.execute(
                "INSERT INTO repos(id,node_id,owner,name,description,language,default_branch,stars,forks,updated_at,pinned)
                 VALUES (?1,?2,'forkline-demo',?3,'Generated local fixture','Rust','main',?4,?5,?6,?7)",
                (repo_id, format!("R_{repo_id}"), format!("project-{repo_id:03}"), repo_id * 13, repo_id / 3, NOW, i64::from(repo_id <= 8)),
            )?;
            for number in 1..=ISSUES_PER_REPO {
                let id = (repo_id - 1) * ISSUES_PER_REPO + number;
                let is_pr = i64::from(number % 3 == 0);
                let updated_at = NOW - ((id * 17) % 30_000);
                let title = format!("{} #{number}: fixture item", if is_pr == 1 { "PR" } else { "Issue" });
                let body = format!("Generated local data for project {repo_id}, item {number}.");
                tx.execute(
                    "INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,body_md,author_id,created_at,updated_at,last_viewed_at)
                     VALUES (?1,?2,?3,?4,?5,'open',?6,?7,1,?8,?9,?10)",
                    (id, format!("I_{id}"), repo_id, number, is_pr, title.clone(), body.clone(), updated_at - 86_400, updated_at, (id % 5 == 0).then_some(updated_at)),
                )?;
                if is_pr == 1 {
                    tx.execute(
                        "INSERT INTO pr_details(issue_id,base_ref,head_ref,base_oid,head_oid,additions,deletions,changed_files)
                         VALUES (?1,'main','fixture','0000000','1111111',?2,?3,2)",
                        (id, number * 3, number),
                    )?;
                }
            }
        }
        // Per-area fixtures; each module is owned by one Phase 1 agent.
        repo::seed(&tx)?;
        lists::seed(&tx)?;
        pr::seed(&tx)?;
        explore::seed(&tx, NOW)?;
        git::seed(&tx, &fixture)?;
        // Index after all seed modules, so comments they add are searchable too.
        let ids = tx.prepare("SELECT id FROM issues")?.query_map([], |r| r.get::<_, i64>(0))?.collect::<Result<Vec<_>, _>>()?;
        for id in ids {
            fl_search::index_issue(&tx, id)?;
        }
        tx.commit()
    })?;
    let comments: i64 = db.read(|c| c.query_row("SELECT count(*) FROM comments", [], |r| r.get(0)))?;
    println!(
        "Generated {} issues, {comments} comments across {REPO_COUNT} repositories at {}",
        REPO_COUNT * ISSUES_PER_REPO,
        db_path(&data_dir).display()
    );
    Ok(())
}
