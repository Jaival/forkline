use fl_db::{Db, queries};
use fl_types::{CheckState, IssueState, PageReq, QueueGroup, QueueKind, QueueQuery, QueueRow};

fn fresh() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("t.db")).unwrap();
    (dir, db)
}

/// Repos 1 and 2; issues 1..=6; viewer `me`.
fn seed(db: &Db) {
    db.write(|c| {
        c.execute_batch(
            "INSERT INTO settings VALUES ('viewer', '{\"id\":1,\"login\":\"me\"}');
             INSERT INTO users(id, login) VALUES (1, 'me'), (2, 'bob');
             INSERT INTO repos(id, node_id, owner, name, updated_at) VALUES (1,'R','o','a',0), (2,'R','o','b',0);
             INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,author_id,created_at,updated_at,last_viewed_at) VALUES
               (1,'I',1,1,0,'open','i1',1,0,10,NULL),
               (2,'I',1,2,1,'open','p2',2,0,20,NULL),
               (3,'I',2,1,1,'open','p3',2,0,30,30),
               (4,'I',2,2,0,'closed','i4',1,0,40,NULL),
               (5,'I',2,3,1,'merged','p5',2,0,50,NULL),
               (6,'I',2,4,1,'open','p6',NULL,0,60,0);
             INSERT INTO pr_details(issue_id,base_ref,head_ref,base_oid,head_oid,check_state) VALUES
               (2,'m','h','b','h','SUCCESS'), (3,'m','h','b','h','ERROR'), (6,'m','h','b','h',NULL);
             INSERT INTO pr_review_requests VALUES (3, 'me'), (2, 'bob'), (5, 'me'), (6, 'me');",
        )
    })
    .unwrap();
}

fn seed_inbox(db: &Db) {
    db.write(|c| {
        c.execute_batch(
            "INSERT INTO notifications(id,repo_id,subject_type,issue_id,title,reason,unread,done,updated_at) VALUES
               ('sub',1,'Issue',1,'t','subscribed',1,0,100),
               ('men',1,'Issue',1,'t','mention',1,0,90),
               ('team',2,'PullRequest',3,'t','team_mention',0,0,95),
               ('rev',2,'PullRequest',3,'t','review_requested',1,0,50),
               ('asg',1,'Issue',1,'t','assign',1,0,70),
               ('aut',2,'PullRequest',6,'t','author',1,0,60),
               ('unk',1,'Issue',1,'t','weird_reason',1,0,200),
               ('done',1,'Issue',1,'t','mention',1,1,300),
               ('rel',2,'Release',NULL,'t','subscribed',1,0,10);",
        )
    })
    .unwrap();
}

fn q(kind: QueueKind, repo_id: Option<i64>) -> QueueQuery {
    QueueQuery { kind, repo_id }
}

fn rows(db: &Db, q: QueueQuery) -> Vec<QueueRow> {
    db.read(|c| queries::queue::page(c, &q, None, PageReq { offset: 0, limit: 100 }))
        .unwrap()
        .items
}

fn keys(rows: &[QueueRow]) -> Vec<&str> {
    rows.iter().map(|r| r.key.as_str()).collect()
}

#[test]
fn inbox_groups_and_order() {
    let (_d, db) = fresh();
    seed(&db);
    seed_inbox(&db);
    let r = rows(&db, q(QueueKind::Inbox, None));
    // Done 'done' is excluded. Order: group, then newest first.
    assert_eq!(keys(&r), ["rev", "team", "men", "asg", "aut", "unk", "sub", "rel"]);
    let groups: Vec<_> = r.iter().map(|x| x.group).collect();
    use QueueGroup::*;
    assert_eq!(
        groups,
        [ReviewRequested, Mentioned, Mentioned, Assigned, YourPrs, Other, Other, Other]
    );
    let rev = &r[0];
    assert_eq!(rev.issue_id, Some(3));
    assert_eq!(rev.repo, "o/b");
    assert_eq!(rev.number, Some(1));
    assert!(rev.is_pr);
    assert_eq!(rev.state, Some(IssueState::Open));
    assert_eq!(rev.author.as_deref(), Some("bob"));
    assert_eq!(rev.checks, Some(CheckState::Failure));
    assert!(rev.unread);
    assert!(!r[1].unread);
    // Plain issue: no checks, not a PR.
    let men = &r[2];
    assert!(!men.is_pr && men.checks.is_none());
    assert_eq!(men.author.as_deref(), Some("me"));
    // PR without author or check state.
    let aut = &r[4];
    assert!(aut.author.is_none() && aut.checks.is_none());
}

#[test]
fn inbox_row_without_issue() {
    let (_d, db) = fresh();
    seed(&db);
    seed_inbox(&db);
    let r = rows(&db, q(QueueKind::Inbox, None));
    let rel = r.iter().find(|x| x.key == "rel").unwrap();
    assert!(rel.issue_id.is_none() && rel.number.is_none());
    assert!(rel.state.is_none() && rel.author.is_none() && rel.checks.is_none());
    assert!(!rel.is_pr);
    assert_eq!(rel.title, "t");
}

#[test]
fn inbox_repo_filter() {
    let (_d, db) = fresh();
    seed(&db);
    seed_inbox(&db);
    let r = rows(&db, q(QueueKind::Inbox, Some(2)));
    assert_eq!(keys(&r), ["rev", "team", "aut", "rel"]);
    assert!(r.iter().all(|x| x.repo_id == 2));
}

#[test]
fn pulls_and_issues() {
    let (_d, db) = fresh();
    seed(&db);
    let pulls = rows(&db, q(QueueKind::Pulls, None));
    assert_eq!(keys(&pulls), ["6", "3", "2"]);
    assert!(pulls.iter().all(|r| r.is_pr && r.group == QueueGroup::Open));
    assert_eq!(pulls[0].checks, None);
    assert_eq!(pulls[1].checks, Some(CheckState::Failure));
    assert_eq!(pulls[2].checks, Some(CheckState::Success));
    // p3 was viewed at its update time; p6 at 0 is stale; p2 never viewed.
    assert_eq!(pulls.iter().map(|r| r.unread).collect::<Vec<_>>(), [true, false, true]);
    assert_eq!(pulls[0].author, None);
    assert_eq!(pulls[2].author.as_deref(), Some("bob"));
    assert_eq!(pulls[2].number, Some(2));
    assert_eq!(pulls[2].issue_id, Some(2));

    let issues = rows(&db, q(QueueKind::Issues, None));
    assert_eq!(keys(&issues), ["1"]);
    assert!(!issues[0].is_pr && issues[0].checks.is_none());

    assert_eq!(keys(&rows(&db, q(QueueKind::Pulls, Some(1)))), ["2"]);
    assert!(rows(&db, q(QueueKind::Issues, Some(2))).is_empty());
}

#[test]
fn reviews_only_requested_from_viewer() {
    let (_d, db) = fresh();
    seed(&db);
    let r = rows(&db, q(QueueKind::Reviews, None));
    // p5 is merged, p2 is requested from bob.
    assert_eq!(keys(&r), ["6", "3"]);
    assert!(r.iter().all(|x| x.group == QueueGroup::ReviewRequested));
    assert_eq!(keys(&rows(&db, q(QueueKind::Reviews, Some(1)))), Vec::<&str>::new());
}

#[test]
fn counts_match_rows() {
    let (_d, db) = fresh();
    seed(&db);
    seed_inbox(&db);
    use QueueGroup::*;
    let counts = |query| db.read(|c| queries::queue::counts(c, &query, None)).unwrap();
    assert_eq!(
        counts(q(QueueKind::Inbox, None)),
        [(ReviewRequested, 1), (Mentioned, 2), (Assigned, 1), (YourPrs, 1), (Other, 3)]
    );
    assert_eq!(counts(q(QueueKind::Inbox, Some(2))), [(ReviewRequested, 1), (Mentioned, 1), (YourPrs, 1), (Other, 1)]);
    assert_eq!(counts(q(QueueKind::Pulls, None)), [(Open, 3)]);
    assert_eq!(counts(q(QueueKind::Issues, None)), [(Open, 1)]);
    assert_eq!(counts(q(QueueKind::Reviews, None)), [(ReviewRequested, 2)]);
    assert!(counts(q(QueueKind::Issues, Some(2))).is_empty());
    assert!(counts(q(QueueKind::Reviews, Some(1))).is_empty());
    for kind in [QueueKind::Inbox, QueueKind::Pulls, QueueKind::Issues, QueueKind::Reviews] {
        for repo in [None, Some(1), Some(2)] {
            let query = q(kind, repo);
            let total: u32 = counts(query).iter().map(|c| c.1).sum();
            assert_eq!(total as usize, rows(&db, query).len(), "{query:?}");
        }
    }
    let empty = fresh();
    assert!(empty.1.read(|c| queries::queue::counts(c, &q(QueueKind::Inbox, None), None)).unwrap().is_empty());
}

#[test]
fn paging() {
    let (_d, db) = fresh();
    seed(&db);
    let query = q(QueueKind::Pulls, None);
    let page = |offset| {
        db.read(|c| queries::queue::page(c, &query, None, PageReq { offset, limit: 2 })).unwrap()
    };
    let first = page(0);
    assert_eq!(keys(&first.items), ["6", "3"]);
    assert_eq!(first.next, Some(2));
    let second = page(2);
    assert_eq!(keys(&second.items), ["2"]);
    assert_eq!(second.next, None);
}

fn inbox_keys(db: &Db) -> Vec<String> {
    rows(db, q(QueueKind::Inbox, None)).into_iter().map(|r| r.key).collect()
}

fn unread(db: &Db) -> u32 {
    db.read(queries::inbox::unread_count).unwrap()
}

#[test]
fn done_hides_and_returns_on_later_update() {
    let (_d, db) = fresh();
    seed(&db);
    seed_inbox(&db);
    let before = unread(&db);
    assert!(db.write(|c| queries::inbox::set_done(c, "sub", 1000)).unwrap());
    assert!(!inbox_keys(&db).contains(&"sub".to_string()));
    assert_eq!(unread(&db), before - 1);
    let page = db
        .read(|c| queries::inbox::list(c, &fl_types::InboxFilter::default(), PageReq { offset: 0, limit: 100 }))
        .unwrap();
    assert!(page.items.iter().all(|r| r.id != "sub"));
    // done_at = MAX(now, updated_at): a future-dated row is hidden too.
    assert!(db.write(|c| queries::inbox::set_done(c, "unk", 5)).unwrap());
    assert!(!inbox_keys(&db).contains(&"unk".to_string()));
    // A later update brings it back.
    db.write(|c| c.execute("UPDATE notifications SET updated_at = 1001 WHERE id = 'sub'", []))
        .unwrap();
    assert!(inbox_keys(&db).contains(&"sub".to_string()));
    assert!(!db.write(|c| queries::inbox::set_done(c, "missing", 1)).unwrap());
}

#[test]
fn snooze_hides_until_time_passes() {
    let (_d, db) = fresh();
    seed(&db);
    seed_inbox(&db);
    let before = unread(&db);
    assert!(db.write(|c| queries::inbox::snooze(c, "men", 4_000_000_000)).unwrap());
    assert!(!inbox_keys(&db).contains(&"men".to_string()));
    assert_eq!(unread(&db), before - 1);
    assert!(db.write(|c| queries::inbox::snooze(c, "men", 1)).unwrap());
    assert!(inbox_keys(&db).contains(&"men".to_string()));
    assert_eq!(unread(&db), before);
    // Done clears a snooze.
    db.write(|c| queries::inbox::snooze(c, "asg", 4_000_000_000)).unwrap();
    db.write(|c| queries::inbox::set_done(c, "asg", 1)).unwrap();
    let until: Option<i64> = db
        .read(|c| c.query_row("SELECT snoozed_until FROM notifications WHERE id = 'asg'", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(until, None);
}
