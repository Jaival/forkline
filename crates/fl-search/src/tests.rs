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
