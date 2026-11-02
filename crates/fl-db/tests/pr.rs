use fl_db::{Db, queries::pr};
use fl_types::{DiffSide, PageReq};

fn seeded() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("t.db")).unwrap();
    db.write(|c| {
        c.execute_batch(
            "INSERT INTO repos (id, node_id, owner, name, updated_at) VALUES (1, 'R', 'o', 'r', 0);
             INSERT INTO users (id, login) VALUES (1, 'ana'), (2, 'bo');
             INSERT INTO issues (id, node_id, repo_id, number, is_pr, state, title, author_id, created_at, updated_at)
               VALUES (10, 'I', 1, 5, 1, 'open', 'Add thing', 1, 0, 100),
                      (11, 'J', 1, 6, 0, 'open', 'An issue', 1, 0, 100);
             INSERT INTO pr_details (issue_id, base_ref, head_ref, base_oid, head_oid, mergeable, check_state)
               VALUES (10, 'main', 'topic', 'aaa', 'bbb', 'MERGEABLE', 'SUCCESS');
             INSERT INTO pr_commits (pr_id, oid, position) VALUES (10, 'c2', 1), (10, 'c1', 0);
             INSERT INTO commits (repo_id, oid, headline, authored_at) VALUES (1, 'c1', 'First', 5);
             INSERT INTO checks (id, repo_id, commit_oid, name, status, conclusion)
               VALUES (1, 1, 'bbb', 'build', 'COMPLETED', 'SUCCESS'), (2, 1, 'old', 'build', 'COMPLETED', 'FAILURE');
             INSERT INTO comments (id, issue_id, author_id, body_md, created_at, updated_at)
               VALUES (-1, 10, 2, 'local', 9, 9), (1, 10, 1, 'first', 1, 1);
             INSERT INTO reviews (id, pr_id, author_id, state, submitted_at) VALUES (1, 10, 2, 'APPROVED', 3);
             INSERT INTO review_threads (id, node_id, pr_id, path, side, line, original_line, is_resolved, is_outdated)
               VALUES (1, 'T1', 10, 'b.rs', 'LEFT', 4, 4, 0, 0), (2, 'T2', 10, 'a.rs', 'RIGHT', NULL, 7, 1, 1);
             INSERT INTO review_comments (id, thread_id, author_id, body_md, created_at)
               VALUES (2, 1, 2, 'second', 2), (1, 1, 1, 'first', 1);
             INSERT INTO action_queue (id, kind, payload_json, target_table, target_id, next_attempt_at, created_at)
               VALUES (1, 'resolve_thread', '{}', 'review_threads', 1, 0, 0);
             INSERT INTO pending_fields (table_name, row_id, field, action_id) VALUES ('review_threads', 1, 'is_resolved', 1);",
        )
    })
    .unwrap();
    (dir, db)
}

#[test]
fn get_returns_pr_with_its_parts_in_order() {
    let (_d, db) = seeded();
    let d = db.read(|c| pr::get(c, 10)).unwrap().unwrap();
    assert_eq!(d.row.number, 5);
    assert_eq!((d.base_ref.as_str(), d.head_oid.as_str()), ("main", "bbb"));
    assert_eq!(d.comments.iter().map(|c| c.id).collect::<Vec<_>>(), [1, -1]);
    assert_eq!(d.reviews.len(), 1);
    // Only checks on the head commit.
    assert_eq!(d.checks.len(), 1);
    // Commits follow `position`; a commit missing from `commits` has an empty headline.
    assert_eq!(d.commits.iter().map(|c| c.oid.as_str()).collect::<Vec<_>>(), ["c1", "c2"]);
    assert_eq!(d.commits[0].headline, "First");
    assert_eq!(d.commits[1].headline, "");
}

#[test]
fn get_returns_none_for_plain_issues_and_missing_ids() {
    let (_d, db) = seeded();
    assert!(db.read(|c| pr::get(c, 11)).unwrap().is_none());
    assert!(db.read(|c| pr::get(c, 99)).unwrap().is_none());
}

#[test]
fn threads_are_ordered_paged_and_flag_pending() {
    let (_d, db) = seeded();
    let page = db
        .read(|c| pr::threads(c, 10, PageReq { offset: 0, limit: 1 }))
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].path, "a.rs");
    assert_eq!(page.items[0].line, None);
    assert_eq!(page.next, Some(1));
    let rest = db
        .read(|c| pr::threads(c, 10, PageReq { offset: 1, limit: 1 }))
        .unwrap();
    let t = &rest.items[0];
    assert_eq!((t.side, t.line, t.pending), (DiffSide::Left, Some(4), true));
    assert_eq!(t.comments.iter().map(|c| c.body_md.as_deref().unwrap()).collect::<Vec<_>>(), ["first", "second"]);
    assert_eq!(rest.next, None);
}

#[test]
fn viewed_files_reset_when_the_head_changes() {
    let (_d, db) = seeded();
    db.write(|c| pr::set_viewed(c, 10, "a.rs", true)).unwrap();
    db.write(|c| pr::set_viewed(c, 10, "b.rs", true)).unwrap();
    db.write(|c| pr::set_viewed(c, 10, "b.rs", false)).unwrap();
    assert_eq!(db.read(|c| pr::viewed_paths(c, 10)).unwrap(), ["a.rs"]);
    db.write(|c| c.execute("UPDATE pr_details SET head_oid = 'new' WHERE issue_id = 10", []))
        .unwrap();
    assert!(db.read(|c| pr::viewed_paths(c, 10)).unwrap().is_empty());
}

#[test]
fn diff_target_has_repo_and_oids() {
    let (_d, db) = seeded();
    let t = db.read(|c| pr::diff_target(c, 10)).unwrap().unwrap();
    assert_eq!((t.owner.as_str(), t.name.as_str(), t.base_oid.as_str(), t.head_oid.as_str()), ("o", "r", "aaa", "bbb"));
}

#[test]
fn pr_queries_use_indexes() {
    let (_d, db) = seeded();
    let queries = [
        "SELECT id FROM review_threads WHERE pr_id = 1 ORDER BY path, id LIMIT 101",
        "SELECT id FROM review_comments WHERE thread_id = 1 ORDER BY created_at, id",
        "SELECT oid FROM pr_commits WHERE pr_id = 1 ORDER BY position",
        "SELECT id FROM reviews WHERE pr_id = 1 ORDER BY submitted_at",
        "SELECT id FROM checks WHERE repo_id = 1 AND commit_oid = 'x' ORDER BY name",
        "SELECT id FROM comments WHERE issue_id = 1 ORDER BY created_at",
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
            assert!(!step.contains("TEMP B-TREE"), "sort step in plan for:\n{q}\nplan: {plan:?}");
        }
    }
}
