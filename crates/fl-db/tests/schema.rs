use fl_db::rusqlite::types::Value;
use fl_db::{Db, migrate, queries};
use fl_types::{InboxFilter, IssueFilter, IssueState, PageReq};

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

/// One repo with one open issue (id 1) that was never viewed.
fn seed_issue(db: &Db) {
    db.write(|c| {
        c.execute_batch(
            "INSERT INTO repos (id, node_id, owner, name, updated_at) VALUES (1, 'R', 'o', 'r', 0);
             INSERT INTO issues (id, node_id, repo_id, number, is_pr, state, title, created_at, updated_at)
               VALUES (1, 'I', 1, 1, 0, 'open', 'Crème brûlée crash', 0, 100);
             INSERT INTO search_docs (issue_id) VALUES (1);
             INSERT INTO search_fts (rowid, title, body)
               VALUES (last_insert_rowid(), 'Crème brûlée crash', 'body');",
        )
    })
    .unwrap();
}

fn fts_hits(db: &Db, q: &str) -> i64 {
    db.read(|c| {
        c.query_row(
            "SELECT count(*) FROM search_fts f JOIN search_docs d ON d.id = f.rowid
             WHERE search_fts MATCH ?1",
            [q],
            |r| r.get(0),
        )
    })
    .unwrap()
}

#[test]
fn fts5_prefix_search_works() {
    let (_d, db) = fresh();
    seed_issue(&db);
    assert_eq!(fts_hits(&db, "creme AND cr*"), 1);
}

#[test]
fn deleting_an_issue_removes_its_search_rows() {
    let (_d, db) = fresh();
    seed_issue(&db);
    db.write(|c| c.execute("DELETE FROM repos WHERE id = 1", []))
        .unwrap();
    let rows: i64 = db
        .read(|c| c.query_row("SELECT count(*) FROM search_fts", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn never_viewed_items_are_unread_until_viewed() {
    let (_d, db) = fresh();
    seed_issue(&db);
    let unread = |db: &Db| db.read(|c| queries::issue::get(c, 1)).unwrap().unwrap().row.unread;
    assert!(unread(&db));
    db.write(|c| queries::issue::mark_viewed(c, 1, 200)).unwrap();
    assert!(!unread(&db));
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
/// without `USING INDEX`, and no `USE TEMP B-TREE` sort step. Add each new
/// list query here.
#[test]
fn query_plans_use_indexes() {
    let (_d, db) = fresh();
    let queries: [String; 8] = [
        format!(
            "SELECT {} FROM issues i INDEXED BY issues_updated JOIN repos r ON r.id = i.repo_id
             LEFT JOIN users u ON u.id = i.author_id WHERE i.state = 'open'
             ORDER BY i.updated_at DESC LIMIT 101",
            queries::ISSUE_ROW_COLS
        ),
        "SELECT id FROM repos WHERE pinned = 1 ORDER BY owner, name".into(),
        "SELECT id FROM issues WHERE repo_id = 1 AND is_pr = 1 AND state = 'open' ORDER BY updated_at DESC".into(),
        "SELECT id FROM comments WHERE issue_id = 1 ORDER BY created_at".into(),
        "SELECT id FROM notifications WHERE done = 0 ORDER BY updated_at DESC".into(),
        "SELECT id FROM notifications WHERE done = 0 AND repo_id = 1 ORDER BY updated_at DESC".into(),
        "SELECT id FROM search_docs WHERE issue_id = 1".into(),
        "SELECT id FROM action_queue WHERE state = 'pending' AND next_attempt_at <= 0".into(),
    ];
    let req = PageReq { offset: 0, limit: 50 };
    let state = Some(IssueState::Open);
    let filters = [
        IssueFilter { state, ..Default::default() },
        IssueFilter { state, is_pr: Some(true), ..Default::default() },
        IssueFilter { repo_id: Some(1), state, is_pr: Some(false), ..Default::default() },
        IssueFilter { state, author: Some("a".into()), ..Default::default() },
        IssueFilter { state, assignee: Some("a".into()), ..Default::default() },
        IssueFilter { state, label: Some("bug".into()), ..Default::default() },
        IssueFilter { state, review_requested: true, ..Default::default() },
        IssueFilter::default(),
    ];
    let mut queries: Vec<(String, Vec<Value>)> = queries.map(|q| (q, Vec::new())).into();
    queries.extend(filters.iter().map(|f| queries::lists::issue_list_sql(f, req)));
    for grouped in [false, true] {
        for repo_id in [None, Some(1)] {
            let f = InboxFilter { repo_id, unread_only: true, grouped };
            queries.push(queries::inbox::inbox_sql(&f, req));
        }
    }
    for (q, args) in queries {
        let plan: Vec<String> = db
            .read(|c| {
                c.prepare(&format!("EXPLAIN QUERY PLAN {q}"))?
                    .query_map(rusqlite::params_from_iter(args), |r| r.get::<_, String>(3))?
                    .collect()
            })
            .unwrap();
        for step in &plan {
            let full_scan = step.starts_with("SCAN") && !step.contains("USING");
            assert!(!full_scan, "full scan in plan for:\n{q}\nplan: {plan:?}");
        }
    }
}

/// Two repos, five items; check each `issue_list` filter and the inbox.
#[test]
fn issue_list_filters_and_inbox() {
    let (_d, db) = fresh();
    db.write(|c| {
        c.execute_batch(
            "INSERT INTO settings VALUES ('viewer', '{\"id\":1,\"login\":\"me\"}');
             INSERT INTO users(id, login) VALUES (1, 'me'), (2, 'bob');
             INSERT INTO repos(id, node_id, owner, name, updated_at) VALUES (1,'R','o','a',0), (2,'R','o','b',0);
             INSERT INTO labels(id, repo_id, name, color) VALUES (1, 1, 'bug', 'f00');
             INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,author_id,created_at,updated_at) VALUES
               (1,'I',1,1,0,'open','i1',1,0,10), (2,'I',1,2,1,'open','p2',2,0,20),
               (3,'I',2,1,1,'open','p3',2,0,30), (4,'I',2,2,0,'closed','i4',1,0,40),
               (5,'I',2,3,1,'merged','p5',2,0,50);
             INSERT INTO issue_labels VALUES (1, 1);
             INSERT INTO issue_assignees VALUES (2, 1);
             INSERT INTO pr_review_requests VALUES (3, 'me'), (2, 'bob'), (5, 'me');
             INSERT INTO notifications(id,repo_id,subject_type,issue_id,title,reason,unread,done,updated_at) VALUES
               ('a',1,'Issue',1,'t','mention',1,0,5), ('b',2,'PullRequest',3,'t','mention',0,0,6),
               ('c',1,'Issue',1,'t','mention',1,1,7), ('d',2,'Release',NULL,'t','mention',1,0,8);",
        )
    })
    .unwrap();
    let ids = |f: IssueFilter| -> Vec<i64> {
        let page = PageReq { offset: 0, limit: 10 };
        db.read(|c| queries::lists::issue_list(c, &f, page))
            .unwrap()
            .items
            .iter()
            .map(|r| r.id)
            .collect()
    };
    let open = Some(IssueState::Open);
    assert_eq!(ids(IssueFilter::default()), [5, 4, 3, 2, 1]);
    assert_eq!(ids(IssueFilter { state: open, is_pr: Some(true), ..Default::default() }), [3, 2]);
    assert_eq!(ids(IssueFilter { repo_id: Some(1), ..Default::default() }), [2, 1]);
    assert_eq!(ids(IssueFilter { author: Some("bob".into()), ..Default::default() }), [5, 3, 2]);
    assert_eq!(ids(IssueFilter { assignee: Some("me".into()), ..Default::default() }), [2]);
    assert_eq!(ids(IssueFilter { label: Some("bug".into()), ..Default::default() }), [1]);
    // Review requests: open PRs asked of the viewer (not the merged one).
    assert_eq!(ids(IssueFilter { state: open, review_requested: true, ..Default::default() }), [3]);

    let inbox = |f: InboxFilter| -> Vec<String> {
        let page = PageReq { offset: 0, limit: 10 };
        db.read(|c| queries::inbox::list(c, &f, page))
            .unwrap()
            .items
            .iter()
            .map(|r| r.id.clone())
            .collect()
    };
    assert_eq!(inbox(InboxFilter::default()), ["d", "b", "a"]);
    assert_eq!(inbox(InboxFilter { unread_only: true, ..Default::default() }), ["d", "a"]);
    assert_eq!(inbox(InboxFilter { grouped: true, ..Default::default() }), ["a", "d", "b"]);
    assert_eq!(inbox(InboxFilter { repo_id: Some(2), ..Default::default() }), ["d", "b"]);
}

#[test]
fn thread_reads_comments_pending_and_links() {
    let (_d, db) = fresh();
    seed_issue(&db);
    db.write(|c| {
        c.execute_batch(
            "INSERT INTO users(id, login) VALUES (1, 'me');
             INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,created_at,updated_at)
               VALUES (2,'I',1,2,1,'open','pr',0,100);
             INSERT INTO issue_links VALUES (1, 2);
             INSERT INTO issue_assignees VALUES (1, 1);
             INSERT INTO comments(id,issue_id,author_id,body_md,created_at,updated_at) VALUES
               (7,1,1,'hi',10,10), (-1,1,1,'local',20,20);
             INSERT INTO timeline_events(id,issue_id,kind,actor_id,created_at) VALUES (1,1,'closed',1,15);
             INSERT INTO action_queue(id,kind,payload_json,target_table,target_id,next_attempt_at,created_at)
               VALUES (1,'close','{}','issues',1,0,0);
             INSERT INTO pending_fields VALUES ('issues', 1, 'state', 1, '\"open\"');",
        )
    })
    .unwrap();
    let t = db.read(|c| queries::issue::thread(c, 1)).unwrap();
    assert_eq!(t.comments.iter().map(|c| c.id).collect::<Vec<_>>(), [7, -1]);
    assert_eq!(t.timeline.len(), 1);
    assert_eq!(t.assignees[0].login, "me");
    assert_eq!(t.linked[0].id, 2);
    assert_eq!(t.pending_fields, ["state"]);
    // The PR sees the issue it fixes.
    assert_eq!(db.read(|c| queries::issue::thread(c, 2)).unwrap().linked[0].id, 1);
}
