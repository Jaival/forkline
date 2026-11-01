use std::path::PathBuf;

use anyhow::{Context, bail};
use fl_db::{Db, db_path};

const REPO_COUNT: i64 = 100;
const ISSUES_PER_REPO: i64 = 100;
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
    let db = Db::open(&db_path(&data_dir)).context("open fake database")?;
    db.write(|conn| {
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM search_fts", [])?;
        tx.execute("DELETE FROM repos", [])?;
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
                    (id, format!("I_{id}"), repo_id, number, is_pr, title.clone(), body.clone(), updated_at - 86_400, updated_at, if id % 5 == 0 { updated_at } else { 0 }),
                )?;
                tx.execute(
                    "INSERT INTO search_fts(title,body,issue_id,kind) VALUES (?1,?2,?3,'issue')",
                    (title, body, id),
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
        tx.commit()
    })?;
    println!(
        "Generated {} issues across {REPO_COUNT} repositories at {}",
        REPO_COUNT * ISSUES_PER_REPO,
        db_path(&data_dir).display()
    );
    Ok(())
}
