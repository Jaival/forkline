use fl_db::{Db, queries::explore};
use fl_types::PageReq;

fn seeded() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("t.db")).unwrap();
    db.write(|c| {
        c.execute_batch(
            "INSERT INTO users (id, login) VALUES (1, 'ann'), (2, 'bob'), (3, 'cy'), (4, 'di'), (5, 'ed');
             INSERT INTO following (user_id) VALUES (1), (2), (3), (4), (5);
             INSERT INTO repos (id, node_id, owner, name, updated_at, starred, explore_only, stars)
               VALUES (1, 'R1', 'o', 'one', 0, 0, 1, 7), (2, 'R2', 'o', 'two', 0, 1, 0, 9), (3, 'R3', 'o', 'three', 0, 0, 1, 1);
             INSERT INTO explore_stars (user_id, repo_id, starred_at) VALUES
               (1, 1, 10), (2, 1, 50), (3, 1, 30), (4, 1, 20), (5, 1, 40),
               (1, 2, 60),
               (2, 3, 5);",
        )
    })
    .unwrap();
    (dir, db)
}

#[test]
fn feed_orders_by_newest_star_and_caps_the_logins() {
    let (_d, db) = seeded();
    let page = db.read(|c| explore::feed(c, PageReq { offset: 0, limit: 10 })).unwrap();
    let names: Vec<_> = page.items.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["two", "one", "three"]);
    let one = &page.items[1];
    assert_eq!(one.starred_by, ["bob", "ed", "cy"]);
    assert_eq!(one.starred_by_count, 5);
    assert_eq!(one.last_starred_at, 50);
    assert!(page.items[0].starred);
    assert_eq!(page.next, None);
}

#[test]
fn feed_pages() {
    let (_d, db) = seeded();
    let first = db.read(|c| explore::feed(c, PageReq { offset: 0, limit: 2 })).unwrap();
    assert_eq!(first.items.len(), 2);
    assert_eq!(first.next, Some(2));
    let second = db.read(|c| explore::feed(c, PageReq { offset: 2, limit: 2 })).unwrap();
    assert_eq!(second.items.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["three"]);
    assert_eq!(second.next, None);
}

#[test]
fn unfollowing_drops_the_stars() {
    let (_d, db) = seeded();
    db.write(|c| c.execute("DELETE FROM following WHERE user_id = 2", [])).unwrap();
    let page = db.read(|c| explore::feed(c, PageReq { offset: 0, limit: 10 })).unwrap();
    let names: Vec<_> = page.items.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["two", "one"]);
    assert_eq!(page.items[1].starred_by_count, 4);
    assert_eq!(page.items[1].last_starred_at, 40);
}

#[test]
fn pinning_an_explore_only_repo_syncs_it() {
    let (_d, db) = seeded();
    db.write(|c| c.execute("UPDATE repos SET sync_enabled = 0 WHERE explore_only = 1", [])).unwrap();
    db.write(|c| fl_db::queries::repo::set_pinned(c, 1, true)).unwrap();
    let flags = |id: i64| {
        db.read(|c| {
            c.query_row("SELECT pinned, explore_only, sync_enabled FROM repos WHERE id = ?1", [id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
            })
        })
        .unwrap()
    };
    assert_eq!(flags(1), (1, 0, 1));
    let page = db.read(|c| explore::feed(c, PageReq { offset: 0, limit: 10 })).unwrap();
    assert!(page.items.iter().find(|r| r.repo_id == 1).unwrap().pinned);
    // Unpinning leaves the flags to sync.
    db.write(|c| fl_db::queries::repo::set_pinned(c, 1, false)).unwrap();
    assert_eq!(flags(1), (0, 0, 1));
    // A repo the user turned sync off for keeps it off when pinned.
    db.write(|c| c.execute("UPDATE repos SET sync_enabled = 0 WHERE id = 2", [])).unwrap();
    db.write(|c| fl_db::queries::repo::set_pinned(c, 2, true)).unwrap();
    assert_eq!(flags(2), (1, 0, 0));
}
