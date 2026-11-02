use fl_db::{Db, queries::repo};
use fl_types::{PageReq, RepoFilter};

fn seeded() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("t.db")).unwrap();
    db.write(|c| {
        c.execute_batch(
            "INSERT INTO users (id, login) VALUES (1, 'octocat');
             INSERT INTO repos (id, node_id, owner, name, updated_at, starred, languages_json)
               VALUES (1, 'R1', 'acme', 'rocket', 0, 1, '[{\"name\":\"Rust\",\"bytes\":10}]'),
                      (2, 'R2', 'acme', 'a_b', 0, 0, 'not json'),
                      (3, 'R3', 'zed', 'tool', 0, 1, NULL);
             INSERT INTO issues (id, node_id, repo_id, number, is_pr, state, title, created_at, updated_at)
               VALUES (1, 'I1', 1, 1, 1, 'open', 'pr', 0, 1), (2, 'I2', 1, 2, 0, 'open', 'bug', 0, 1),
                      (3, 'I3', 1, 3, 0, 'closed', 'old', 0, 1);
             INSERT INTO commits (repo_id, oid, headline, authored_at, on_default_branch)
               VALUES (1, 'a', 'one', 10, 1), (1, 'b', 'two', 20, 1), (1, 'c', 'side', 30, 0);
             INSERT INTO repo_contributors (repo_id, user_id, contributions) VALUES (1, 1, 5);
             INSERT INTO star_history (repo_id, day, stars) VALUES (1, 2, 20), (1, 1, 10);",
        )
    })
    .unwrap();
    (dir, db)
}

fn names(db: &Db, query: &str, starred_only: bool, req: PageReq) -> (Vec<String>, Option<u32>) {
    let f = RepoFilter { query: query.into(), starred_only };
    let p = db.read(|c| repo::list(c, &f, req)).unwrap();
    (p.items.into_iter().map(|r| r.name).collect(), p.next)
}

#[test]
fn list_filters_and_pages() {
    let (_d, db) = seeded();
    let all = PageReq { offset: 0, limit: 10 };
    assert_eq!(names(&db, "", false, all).0, ["a_b", "rocket", "tool"]);
    assert_eq!(names(&db, "", true, all).0, ["rocket", "tool"]);
    assert_eq!(names(&db, "ACME/", false, all).0, ["a_b", "rocket"]);
    // `_` is literal, not a wildcard.
    assert_eq!(names(&db, "a_b", false, all).0, ["a_b"]);
    assert_eq!(names(&db, "%", false, all).0, Vec::<String>::new());
    assert_eq!(names(&db, "", false, PageReq { offset: 0, limit: 2 }), (vec!["a_b".into(), "rocket".into()], Some(2)));
    assert_eq!(names(&db, "", false, PageReq { offset: 2, limit: 2 }), (vec!["tool".into()], None));
}

#[test]
fn pin_toggle_and_missing_repo() {
    let (_d, db) = seeded();
    assert!(db.write(|c| repo::set_pinned(c, 3, true)).unwrap());
    assert_eq!(db.read(repo::pinned).unwrap().len(), 1);
    assert!(db.write(|c| repo::set_pinned(c, 3, false)).unwrap());
    assert!(db.read(repo::pinned).unwrap().is_empty());
    assert!(!db.write(|c| repo::set_pinned(c, 99, true)).unwrap());
}

#[test]
fn overview_collects_sections() {
    let (_d, db) = seeded();
    let o = db.read(|c| repo::overview(c, 1)).unwrap().unwrap();
    assert_eq!((o.open_prs, o.open_issues), (1, 1));
    assert_eq!(o.languages.len(), 1);
    // Default branch only, newest first.
    assert_eq!(o.commits.iter().map(|c| c.oid.as_str()).collect::<Vec<_>>(), ["b", "a"]);
    assert_eq!(o.contributors[0].login, "octocat");
    // Oldest first for the sparkline.
    assert_eq!(o.star_history.iter().map(|s| s.stars).collect::<Vec<_>>(), [10, 20]);
    // Bad languages JSON gives an empty bar, not an error.
    assert!(db.read(|c| repo::overview(c, 2)).unwrap().unwrap().languages.is_empty());
    assert!(db.read(|c| repo::overview(c, 99)).unwrap().is_none());
}

#[test]
fn list_plans_do_not_scan_issues() {
    let (_d, db) = seeded();
    for sql in [repo::LIST_SQL_ALL, repo::LIST_SQL_STARRED] {
        let plan: Vec<String> = db
            .read(|c| {
                c.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?
                    .query_map(("%", 101, 0), |r| r.get::<_, String>(3))?
                    .collect()
            })
            .unwrap();
        for step in &plan {
            assert!(!(step.starts_with("SCAN") && !step.contains("USING")), "full scan: {plan:?}");
        }
    }
}
