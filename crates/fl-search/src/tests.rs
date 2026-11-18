use super::*;

const REQ: PageReq = PageReq { offset: 0, limit: 50 };

fn seeded() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
    fl_db::migrate::run(&mut conn).unwrap();
    conn.execute_batch(
        "INSERT INTO repos(id,node_id,owner,name,updated_at) VALUES (1,'R','acme','web',0),(2,'R2','acme','api',0);
         INSERT INTO users(id,login) VALUES (1,'octocat'),(2,'hubot');
         INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,body_md,author_id,created_at,updated_at) VALUES
          (1,'a',1,1,0,'open','Crash on startup','Stack overflow in the parser',1,0,100),
          (2,'b',1,2,1,'merged','Fix parser crash','Closes #1',2,0,200),
          (3,'c',2,1,0,'closed','Slow query planner','nothing about it',1,0,300),
          (4,'d',2,2,0,'open','Docs','Mentions the frobnicator only in a comment',2,0,400);
         INSERT INTO comments(id,issue_id,author_id,body_md,created_at,updated_at) VALUES
          (10,4,1,'The frobnicator is broken',0,0),(-1,4,1,'local draft about frobnicator',0,0);
         INSERT INTO labels(id,repo_id,name,color) VALUES (1,1,'Bug','f00');
         INSERT INTO issue_labels VALUES (1,1);
         INSERT INTO issue_assignees VALUES (1,2);",
    )
    .unwrap();
    for id in 1..=4 {
        index_issue(&conn, id).unwrap();
    }
    conn
}

fn ids(conn: &Connection, q: &str) -> Vec<i64> {
    search(conn, q, REQ).unwrap().items.iter().map(|h| h.row.id).collect()
}

#[test]
fn free_text_prefix_and_ranking() {
    let c = seeded();
    let crash = ids(&c, "crash");
    assert_eq!(crash.len(), 2);
    assert_eq!(ids(&c, "star"), vec![1]);
    assert_eq!(ids(&c, "fix pars"), vec![2]);
    assert!(ids(&c, "nonexistentword").is_empty());
    // A title match outranks a body-only match.
    assert_eq!(ids(&c, "parser")[0], 2);
}

#[test]
fn operators_filter_and_negate() {
    let c = seeded();
    assert_eq!(ids(&c, "crash is:pr"), vec![2]);
    assert_eq!(ids(&c, "crash is:issue"), vec![1]);
    assert_eq!(ids(&c, "crash -is:merged"), vec![1]);
    assert_eq!(ids(&c, "crash author:HUBOT"), vec![2]);
    assert_eq!(ids(&c, "crash label:bug"), vec![1]);
    assert_eq!(ids(&c, "crash assignee:hubot"), vec![1]);
    assert_eq!(ids(&c, "repo:acme/api"), vec![4, 3]);
    assert_eq!(ids(&c, "is:open repo:acme/api"), vec![4]);
    assert_eq!(ids(&c, "is:closed"), vec![3]);
    assert!(ids(&c, "crash -parser").is_empty());
    assert_eq!(ids(&c, "crash -startup"), vec![2]);
}

#[test]
fn comment_hit_groups_by_issue_with_markers() {
    let c = seeded();
    let p = search(&c, "frobnicator", REQ).unwrap();
    assert_eq!(p.items.len(), 1, "grouped by issue");
    assert!(p.items[0].snippet.contains('\u{e000}') && p.items[0].snippet.contains('\u{e001}'));
}

#[test]
fn hostile_input_never_errors() {
    let c = seeded();
    for q in [
        "\"", "\"\"\"", "AND OR NOT", "NEAR(", "a*b", "title:foo", "-", "(", "^", "repo:", "\u{e000}",
        "'; DROP TABLE issues; --", "",
    ] {
        search(&c, q, REQ).unwrap_or_else(|e| panic!("{q:?}: {e}"));
    }
}

#[test]
fn reindex_replaces_rows_and_paging_works() {
    let c = seeded();
    c.execute("UPDATE issues SET title = 'Renamed thing' WHERE id = 1", []).unwrap();
    index_issue(&c, 1).unwrap();
    assert!(ids(&c, "startup").is_empty());
    assert_eq!(ids(&c, "renamed"), vec![1]);
    let n: i64 = c
        .query_row("SELECT count(*) FROM search_docs WHERE issue_id = 4", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3);
    let p1 = search(&c, "is:open", PageReq { offset: 0, limit: 1 }).unwrap();
    assert_eq!(p1.items.len(), 1);
    assert_eq!(p1.next, Some(1));
}

#[test]
fn terms_may_hit_different_docs_of_one_issue() {
    let c = seeded();
    c.execute_batch(
        "INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,body_md,author_id,created_at,updated_at)
           VALUES (5,'e',1,5,0,'open','Crash on save','plain body',1,0,500);
         INSERT INTO comments(id,issue_id,author_id,body_md,created_at,updated_at)
           VALUES (20,5,1,'Only happens on windows',0,0),(21,5,1,'Also on linux',0,0);",
    )
    .unwrap();
    index_issue(&c, 5).unwrap();
    // title + comment, comment + comment, body + comment
    assert_eq!(ids(&c, "crash windows"), vec![5]);
    assert_eq!(ids(&c, "windows linux"), vec![5]);
    assert_eq!(ids(&c, "plain windows"), vec![5]);
    // every term must hit some doc of the issue
    assert!(ids(&c, "crash macos").is_empty());
    assert!(ids(&c, "windows nonexistentword").is_empty());
    // a term in a different issue does not count
    assert!(ids(&c, "save frobnicator").is_empty());
    // operators still filter, negation spans docs
    assert_eq!(ids(&c, "crash windows is:issue"), vec![5]);
    assert!(ids(&c, "crash windows is:pr").is_empty());
    assert!(ids(&c, "crash windows author:hubot").is_empty());
    assert!(ids(&c, "crash windows -linux").is_empty());
    assert_eq!(ids(&c, "crash windows -macos"), vec![5]);
}

#[test]
fn snippet_comes_from_a_matching_doc() {
    let c = seeded();
    c.execute_batch(
        "INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,body_md,author_id,created_at,updated_at)
           VALUES (5,'e',1,5,0,'open','Crash on save','plain body',1,0,500);
         INSERT INTO comments(id,issue_id,author_id,body_md,created_at,updated_at)
           VALUES (20,5,1,'Only happens on windows',0,0),(21,5,1,'windows crash again',0,0);",
    )
    .unwrap();
    index_issue(&c, 5).unwrap();
    let hit = |q: &str| search(&c, q, REQ).unwrap().items.remove(0);
    // Comment 20 matches two of three terms, the title only one.
    let h = search(&c, "save happens windows", REQ).unwrap().items.remove(0);
    assert_eq!(h.comment_id, Some(20));
    assert!(h.snippet.contains("\u{e000}happens\u{e001}"));
    // Comment 21 matches both terms, so it wins over the title.
    let h = hit("crash windows");
    assert_eq!(h.comment_id, Some(21));
    assert!(h.snippet.contains('\u{e000}'));
}

mod queue_filter {
    use fl_db::queries::queue;
    use fl_types::{PAGE_MAX, QueueKind, QueueQuery};

    use super::*;

    const ISSUES: QueueQuery = QueueQuery { kind: QueueKind::Issues, repo_id: None };

    fn keys(c: &Connection, q: QueueQuery, filter: &str) -> Vec<String> {
        let cond = crate::queue_filter(c, filter).unwrap();
        let rows = queue::page(c, &q, cond.as_ref(), PageReq { offset: 0, limit: PAGE_MAX }).unwrap().items;
        let total: u32 = queue::counts(c, &q, cond.as_ref()).unwrap().iter().map(|(_, n)| n).sum();
        assert_eq!(total as usize, rows.len(), "counts for {filter:?}");
        rows.into_iter().map(|r| r.key).collect()
    }

    #[test]
    fn empty_query_filters_nothing() {
        let c = seeded();
        assert!(crate::queue_filter(&c, "").unwrap().is_none());
        assert!(crate::queue_filter(&c, "   ").unwrap().is_none());
        assert_eq!(keys(&c, ISSUES, ""), ["4", "1"]);
    }

    #[test]
    fn text_matches_title_body_and_comments() {
        let c = seeded();
        assert_eq!(keys(&c, ISSUES, "crash"), ["1"]);
        assert_eq!(keys(&c, ISSUES, "STACK pars"), ["1"]);
        assert_eq!(keys(&c, ISSUES, "frobnicator"), ["4"]);
        assert_eq!(keys(&c, ISSUES, "frob"), ["4"]);
        assert!(keys(&c, ISSUES, "nonexistentword").is_empty());
        assert_eq!(keys(&c, ISSUES, "-frobnicator"), ["1"]);
    }

    #[test]
    fn operators_and_negation() {
        let c = seeded();
        assert_eq!(keys(&c, ISSUES, "author:hubot"), ["4"]);
        assert_eq!(keys(&c, ISSUES, "-author:hubot"), ["1"]);
        assert_eq!(keys(&c, ISSUES, "label:bug"), ["1"]);
        assert_eq!(keys(&c, ISSUES, "repo:acme/api"), ["4"]);
        assert_eq!(keys(&c, ISSUES, "assignee:hubot crash"), ["1"]);
        assert!(keys(&c, ISSUES, "is:pr").is_empty());
        let pulls = QueueQuery { kind: QueueKind::Pulls, repo_id: None };
        assert!(keys(&c, pulls, "").is_empty(), "the only PR is merged");
    }

    #[test]
    fn broad_terms_filter_per_row() {
        let c = seeded();
        // More than BROAD_DOCS docs hold "common"; every 7th also "rare".
        c.execute_batch(
            "WITH RECURSIVE n(x) AS (SELECT 100 UNION ALL SELECT x + 1 FROM n WHERE x < 2200)
             INSERT INTO issues(id,node_id,repo_id,number,is_pr,state,title,body_md,author_id,created_at,updated_at)
             SELECT x, 'n' || x, 1, x, 0, 'open', 'common ' || x, CASE WHEN x % 7 = 0 THEN 'rare' END, 1, 0, x
             FROM n;",
        )
        .unwrap();
        for id in 100..=2200 {
            index_issue(&c, id).unwrap();
        }
        let cond = crate::queue_filter(&c, "common").unwrap().unwrap();
        assert!(cond.sql.contains("EXISTS"), "{}", cond.sql);
        let both = |q: &str| {
            let cond = crate::queue_filter(&c, q).unwrap();
            let total: u32 = queue::counts(&c, &ISSUES, cond.as_ref()).unwrap().iter().map(|(_, n)| n).sum();
            total
        };
        assert_eq!(both("common"), 2101);
        assert_eq!(both("common rare"), 300);
        assert_eq!(both("common -rare"), 1801);
        assert_eq!(keys(&c, ISSUES, "comm crash").len(), 0);
        let cond = crate::queue_filter(&c, "rare common").unwrap();
        let page = queue::page(&c, &ISSUES, cond.as_ref(), PageReq { offset: 0, limit: 2 }).unwrap();
        let first: Vec<_> = page.items.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(first, ["2198", "2191"]);
    }

    #[test]
    fn unsearchable_text_matches_nothing() {
        let c = seeded();
        assert!(keys(&c, ISSUES, "***").is_empty());
        // Quotes in text never break the SQL.
        assert!(keys(&c, ISSUES, r#"" OR 1=1 --"#).is_empty());
    }

    #[test]
    fn inbox_rows_need_a_cached_issue() {
        let c = seeded();
        c.execute_batch(
            "INSERT INTO notifications(id,repo_id,subject_type,issue_id,title,reason,unread,done,updated_at) VALUES
               ('1',1,'Issue',1,'Crash on startup','mention',1,0,10),
               ('2',2,'Issue',4,'Docs','mention',1,0,20),
               ('3',1,'Release',NULL,'crash release','subscribed',1,0,30);",
        )
        .unwrap();
        let inbox = QueueQuery { kind: QueueKind::Inbox, repo_id: None };
        assert_eq!(keys(&c, inbox, ""), ["2", "1", "3"]);
        assert_eq!(keys(&c, inbox, "crash"), ["1"]);
        assert_eq!(keys(&c, inbox, "repo:acme/api"), ["2"]);
        let in_api = QueueQuery { kind: QueueKind::Inbox, repo_id: Some(2) };
        assert!(keys(&c, in_api, "crash").is_empty());
    }
}
