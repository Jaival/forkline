use fl_db::{Db, queries::views};
use fl_types::QueueKind;

#[test]
fn save_list_replace_delete() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("t.db")).unwrap();
    db.write(|c| {
        let a = views::save(c, QueueKind::Pulls, "mine", "author:me", 1)?;
        views::save(c, QueueKind::Inbox, "bugs", "label:bug", 2)?;
        views::save(c, QueueKind::Pulls, "Big", "is:open", 3)?;
        // Same name and kind: replaced, same id.
        assert_eq!(views::save(c, QueueKind::Pulls, "mine", "author:me is:open", 4)?, a);
        // Same name, other kind: a new view.
        views::save(c, QueueKind::Issues, "mine", "author:me", 5)?;
        let list = views::list(c)?;
        let got: Vec<_> = list.iter().map(|v| (v.kind, v.name.as_str(), v.query.as_str())).collect();
        assert_eq!(
            got,
            [
                (QueueKind::Inbox, "bugs", "label:bug"),
                (QueueKind::Pulls, "Big", "is:open"),
                (QueueKind::Pulls, "mine", "author:me is:open"),
                (QueueKind::Issues, "mine", "author:me"),
            ]
        );
        views::delete(c, a)?;
        assert_eq!(views::list(c)?.len(), 3);
        Ok(())
    })
    .unwrap();
}
