use fl_db::{Db, migrate, queries};
use fl_types::PageReq;

fn fresh() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("t.db")).unwrap();
    (dir, db)
}

#[test]
fn migrates_to_latest_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.db");
    drop(Db::open(&path).unwrap());
    let db = Db::open(&path).unwrap();
    let v: i64 = db
        .read(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(v, migrate::LATEST);
}

#[test]
fn pragmas_are_set() {
    let (_d, db) = fresh();
    let mode: String = db
        .read(|c| c.query_row("PRAGMA journal_mode", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(mode, "wal");
}

#[test]
fn fts5_prefix_search_works() {
    let (_d, db) = fresh();
    db.write(|c| {
        c.execute(
            "INSERT INTO search_fts (title, body, issue_id, kind) VALUES ('Crème brûlée crash', 'body', 1, 'issue')",
            [],
        )
    })
    .unwrap();
    let n: i64 = db
        .read(|c| {
            c.query_row(
                "SELECT count(*) FROM search_fts WHERE search_fts MATCH 'creme AND cr*'",
                [],
                |r| r.get(0),
            )
        })
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn layout_and_settings_roundtrip() {
    let (_d, db) = fresh();
    db.write(|c| queries::layout::save(c, r#"{"v":1}"#))
        .unwrap();
    db.write(|c| queries::layout::save(c, r#"{"v":2}"#))
        .unwrap();
    assert_eq!(
        db.read(queries::layout::get).unwrap().as_deref(),
        Some(r#"{"v":2}"#)
    );
    db.write(|c| queries::layout::set_setting(c, "theme", r#""dark""#))
        .unwrap();
    assert_eq!(
        db.read(|c| queries::layout::setting(c, "theme"))
            .unwrap()
            .as_deref(),
        Some(r#""dark""#)
    );
}

#[test]
fn empty_queries_succeed() {
    let (_d, db) = fresh();
    let p = db
        .read(|c| {
            queries::lists::home_feed(
                c,
                PageReq {
                    offset: 0,
                    limit: 500,
                },
            )
        })
        .unwrap();
    assert!(p.items.is_empty() && p.next.is_none());
    assert!(db.read(|c| queries::issue::get(c, 1)).unwrap().is_none());
    assert!(db.read(queries::repo::pinned).unwrap().is_empty());
    assert_eq!(db.read(queries::sync::status).unwrap().pending_actions, 0);
}

/// Every list query must be served by an index: no full `SCAN <table>`
/// without `USING INDEX`. Add each new list query here.
#[test]
fn query_plans_use_indexes() {
    let (_d, db) = fresh();
    let queries = [
        format!(
            "SELECT {} FROM issues i INDEXED BY issues_updated JOIN repos r ON r.id = i.repo_id
             LEFT JOIN users u ON u.id = i.author_id WHERE i.state = 'open'
             ORDER BY i.updated_at DESC LIMIT 101",
            queries::ISSUE_ROW_COLS
        ),
        "SELECT id FROM repos WHERE pinned = 1 ORDER BY owner, name".into(),
        "SELECT id FROM issues WHERE repo_id = 1 AND is_pr = 1 AND state = 'open' ORDER BY updated_at DESC".into(),
        "SELECT id FROM comments WHERE issue_id = 1 ORDER BY created_at".into(),
        "SELECT id FROM notifications WHERE done = 0 ORDER BY done, repo_id, updated_at DESC".into(),
        "SELECT id FROM action_queue WHERE state = 'pending' AND next_attempt_at <= 0".into(),
    ];
    for q in queries {
        let plan: Vec<String> = db
            .read(|c| {
                c.prepare(&format!("EXPLAIN QUERY PLAN {q}"))?
                    .query_map([], |r| r.get::<_, String>(3))?
                    .collect()
            })
            .unwrap();
        for step in &plan {
            let full_scan = step.starts_with("SCAN") && !step.contains("USING");
            assert!(!full_scan, "full scan in plan for:\n{q}\nplan: {plan:?}");
        }
    }
}
