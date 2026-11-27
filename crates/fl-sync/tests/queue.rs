mod common;

use std::time::{Duration, Instant};

use fl_sync::Error;
use fl_types::{Action, DiffSide, DraftReviewComment, IssueState, ReviewEvent};
use serde_json::json;
use wiremock::matchers::{body_partial_json, body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

fn close(issue_id: i64) -> Action {
    Action::SetState { issue_id, state: IssueState::Closed }
}

#[tokio::test(flavor = "multi_thread")]
async fn comment_is_local_at_once_and_gets_github_id() {
    let env = common::env().await;
    Mock::given(method("POST"))
        .and(path("/repos/o/r/issues/1/comments"))
        .and(body_partial_json(json!({ "body": "hi" })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": 555, "body_html": "<p>hi</p>" })))
        .expect(1)
        .mount(&env.server)
        .await;

    env.engine.enqueue(Action::Comment { issue_id: 1, body_md: "hi".into() }).await.unwrap();
    assert_eq!(env.q::<i64>("SELECT id FROM comments"), -1);
    assert_eq!(env.q::<i64>("SELECT author_id FROM comments"), 10);
    assert_eq!(env.q::<String>("SELECT field FROM pending_fields"), "row");
    assert!(env.emitted.lock().unwrap().contains(&"issues".to_owned()));

    assert_eq!(env.engine.process_actions().await.unwrap(), 1);
    assert_eq!(env.q::<i64>("SELECT id FROM comments"), 555);
    assert_eq!(env.q::<String>("SELECT body_html FROM comments"), "<p>hi</p>");
    assert_eq!(env.q::<i64>("SELECT count(*) FROM pending_fields"), 0);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM action_queue"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn validation_error_rolls_back_and_keeps_failed_row() {
    let env = common::env().await;
    Mock::given(method("PATCH"))
        .and(path("/repos/o/r/issues/1"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({ "message": "Validation Failed" })))
        .expect(2)
        .mount(&env.server)
        .await;

    env.engine.enqueue(close(1)).await.unwrap();
    assert_eq!(env.q::<String>("SELECT state FROM issues WHERE id = 1"), "closed");
    env.engine.process_actions().await.unwrap();

    assert_eq!(env.q::<String>("SELECT state FROM issues WHERE id = 1"), "open");
    assert_eq!(env.q::<i64>("SELECT count(*) FROM pending_fields"), 0);
    let failed = env.engine.failed_actions().await.unwrap();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].summary, "Close o/r#1");
    assert!(failed[0].error.contains("422"), "{}", failed[0].error);
    assert_eq!(env.engine.status().await.unwrap().failed_actions, 1);

    // Retry re-applies the change; discard removes the row.
    env.engine.retry(failed[0].id).await.unwrap();
    assert_eq!(env.q::<String>("SELECT state FROM issues WHERE id = 1"), "closed");
    assert_eq!(env.q::<String>("SELECT state FROM action_queue"), "pending");
    env.engine.process_actions().await.unwrap();
    assert_eq!(env.q::<String>("SELECT state FROM issues WHERE id = 1"), "open");
    env.engine.discard(failed[0].id).await.unwrap();
    assert_eq!(env.q::<i64>("SELECT count(*) FROM action_queue"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn server_error_retries_with_backoff_and_keeps_optimistic_value() {
    let env = common::env().await;
    Mock::given(method("PATCH"))
        .and(path("/repos/o/r/issues/1"))
        .respond_with(ResponseTemplate::new(502))
        .expect(1)
        .mount(&env.server)
        .await;

    env.engine.enqueue(close(1)).await.unwrap();
    env.engine.process_actions().await.unwrap();
    assert_eq!(env.q::<String>("SELECT state FROM issues WHERE id = 1"), "closed");
    assert_eq!(env.q::<String>("SELECT state FROM action_queue"), "pending");
    assert_eq!(env.q::<i64>("SELECT attempts FROM action_queue"), 1);
    assert!(env.q::<i64>("SELECT next_attempt_at - created_at FROM action_queue") >= 2);
    // Not due yet: a second pass sends nothing.
    assert_eq!(env.engine.process_actions().await.unwrap(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn network_error_keeps_action_pending() {
    let env = common::env().await;
    let engine = fl_sync::Engine::new(env.db.clone(), fl_sync::Config::test("http://127.0.0.1:9"));
    engine.set_test_token(Some("t"));
    engine.enqueue(close(1)).await.unwrap();
    engine.process_actions().await.unwrap();
    assert_eq!(env.q::<String>("SELECT state FROM action_queue"), "pending");
    assert_eq!(env.q::<String>("SELECT state FROM issues WHERE id = 1"), "closed");
    assert!(env.q::<String>("SELECT last_error FROM action_queue").starts_with("network"));
}

#[tokio::test(flavor = "multi_thread")]
async fn unauthorized_waits_for_sign_in() {
    let env = common::env().await;
    Mock::given(method("PATCH")).respond_with(ResponseTemplate::new(401)).mount(&env.server).await;
    env.engine.enqueue(close(1)).await.unwrap();
    env.engine.process_actions().await.unwrap();
    assert_eq!(env.q::<String>("SELECT state FROM action_queue"), "pending");
    assert_eq!(env.q::<i64>("SELECT attempts FROM action_queue"), 0);
    assert!(env.engine.token().is_none());
    assert!(!env.engine.auth_state().await.unwrap().signed_in);
}

#[tokio::test(flavor = "multi_thread")]
async fn newer_action_supersedes_unsent_one_and_keeps_confirmed_value() {
    let env = common::env().await;
    let first = env.engine.enqueue(close(1)).await.unwrap();
    let second = env.engine.enqueue(Action::SetState { issue_id: 1, state: IssueState::Open }).await.unwrap();
    assert_ne!(first, second);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM action_queue"), 1);
    assert_eq!(env.q::<i64>("SELECT action_id FROM pending_fields"), second);
    assert_eq!(env.q::<String>("SELECT old_value_json FROM pending_fields"), "\"open\"");

    // An in-flight action is kept; only ownership moves.
    env.db.write(|c| c.execute("UPDATE action_queue SET in_flight = 1", [])).unwrap();
    let third = env.engine.enqueue(close(1)).await.unwrap();
    assert_eq!(env.q::<i64>("SELECT count(*) FROM action_queue"), 2);
    assert_eq!(env.q::<i64>("SELECT action_id FROM pending_fields"), third);
}

#[tokio::test(flavor = "multi_thread")]
async fn labels_and_assignees_roll_back_to_old_sets() {
    let env = common::env().await;
    Mock::given(method("PUT"))
        .and(path("/repos/o/r/issues/1/labels"))
        .and(body_partial_json(json!({ "labels": ["docs"] })))
        .respond_with(ResponseTemplate::new(404))
        .mount(&env.server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/repos/o/r/issues/1"))
        .and(body_partial_json(json!({ "assignees": ["alice"] })))
        .respond_with(ResponseTemplate::new(422))
        .mount(&env.server)
        .await;
    env.engine.enqueue(Action::SetLabels { issue_id: 1, label_ids: vec![101] }).await.unwrap();
    env.engine.enqueue(Action::SetAssignees { issue_id: 1, user_ids: vec![11] }).await.unwrap();
    assert_eq!(env.q::<i64>("SELECT label_id FROM issue_labels"), 101);
    assert_eq!(env.q::<i64>("SELECT user_id FROM issue_assignees"), 11);
    env.engine.process_actions().await.unwrap();
    assert_eq!(env.q::<i64>("SELECT label_id FROM issue_labels"), 100);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM issue_assignees"), 0);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM action_queue WHERE state = 'failed'"), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn review_with_inline_comment_is_local_then_removed_on_failure() {
    let env = common::env().await;
    Mock::given(method("POST"))
        .and(path("/repos/o/r/pulls/2/reviews"))
        .and(body_partial_json(json!({
            "event": "REQUEST_CHANGES",
            "commit_id": "h0",
            "comments": [{ "path": "src/a.rs", "line": 5, "side": "RIGHT", "start_line": 4 }]
        })))
        .respond_with(ResponseTemplate::new(422))
        .expect(1)
        .mount(&env.server)
        .await;
    env.engine
        .enqueue(Action::SubmitReview {
            pr_id: 2,
            event: ReviewEvent::RequestChanges,
            body_md: "fix <this>".into(),
            comments: vec![DraftReviewComment {
                path: "src/a.rs".into(),
                side: DiffSide::Right,
                line: 5,
                start_line: Some(4),
                body_md: "here".into(),
            }],
        })
        .await
        .unwrap();
    assert_eq!(env.q::<String>("SELECT body_md FROM reviews"), "fix <this>");
    assert_eq!(env.q::<Option<String>>("SELECT body_html FROM reviews"), None);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM review_threads WHERE id < 0"), 1);
    assert_eq!(env.q::<i64>("SELECT review_id FROM review_comments"), -1);
    env.engine.process_actions().await.unwrap();
    assert_eq!(env.q::<i64>("SELECT count(*) FROM reviews"), 0);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM review_threads WHERE id < 0"), 0);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM review_comments"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn thread_actions_use_graphql() {
    let env = common::env().await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("resolveReviewThread"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": { "resolveReviewThread": { "thread": { "isResolved": true } } } })))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("addPullRequestReviewThreadReply"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "errors": [{ "message": "nope" }] })))
        .expect(1)
        .mount(&env.server)
        .await;
    env.engine.enqueue(Action::ResolveThread { thread_id: 50, resolved: true }).await.unwrap();
    env.engine.enqueue(Action::ReplyThread { thread_id: 50, body_md: "ok".into() }).await.unwrap();
    assert_eq!(env.q::<i64>("SELECT count(*) FROM review_comments"), 1);
    env.engine.process_actions().await.unwrap();
    assert_eq!(env.q::<i64>("SELECT is_resolved FROM review_threads WHERE id = 50"), 1);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM review_comments"), 0);
    assert_eq!(env.q::<String>("SELECT last_error FROM action_queue"), "GitHub 422: nope");
}

#[tokio::test(flavor = "multi_thread")]
async fn notification_and_merge_actions() {
    let env = common::env().await;
    Mock::given(method("PATCH"))
        .and(path("/notifications/threads/9000"))
        .respond_with(ResponseTemplate::new(205))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/repos/o/r/pulls/2/merge"))
        .and(body_partial_json(json!({ "merge_method": "squash", "sha": "h0" })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "merged": true })))
        .expect(1)
        .mount(&env.server)
        .await;
    env.engine.enqueue(Action::NotificationRead { id: "9000".into() }).await.unwrap();
    env.engine.enqueue(Action::Merge { pr_id: 2, method: fl_types::MergeMethod::Squash }).await.unwrap();
    assert_eq!(env.q::<i64>("SELECT unread FROM notifications"), 0);
    assert_eq!(env.q::<String>("SELECT state FROM issues WHERE id = 2"), "merged");
    assert_eq!(env.engine.process_actions().await.unwrap(), 2);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM action_queue"), 0);
    assert_eq!(env.q::<String>("SELECT state FROM issues WHERE id = 2"), "merged");
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_actions_are_rejected_without_changes() {
    let env = common::env().await;
    let bad = [
        Action::Comment { issue_id: 1, body_md: "  ".into() },
        Action::Comment { issue_id: 999, body_md: "x".into() },
        Action::SetLabels { issue_id: 1, label_ids: vec![200] },
        Action::SetAssignees { issue_id: 1, user_ids: vec![999] },
        Action::SetState { issue_id: 1, state: IssueState::Merged },
        Action::Merge { pr_id: 1, method: fl_types::MergeMethod::Merge },
        Action::SubmitReview { pr_id: 2, event: ReviewEvent::Comment, body_md: String::new(), comments: vec![] },
        Action::ResolveThread { thread_id: 7, resolved: true },
        Action::NotificationDone { id: "abc".into() },
    ];
    for a in bad {
        let r = env.engine.enqueue(a.clone()).await;
        assert!(matches!(r, Err(Error::Invalid(_))), "{a:?} -> {r:?}");
    }
    assert_eq!(env.q::<i64>("SELECT count(*) FROM action_queue"), 0);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM pending_fields"), 0);
}

fn done(id: &str) -> Action {
    Action::NotificationDone { id: id.into() }
}

#[tokio::test(flavor = "multi_thread")]
async fn enqueue_all_skips_a_bad_action_and_retry_all_discard_all_cover_the_failed_list() {
    let env = common::env().await;
    env.db
        .write(|c| {
            c.execute_batch(
                "INSERT INTO notifications (id, repo_id, subject_type, issue_id, title, reason, unread, updated_at)
                 VALUES ('9001', 1, 'Issue', 1, 'Another', 'mention', 1, 100);",
            )
        })
        .unwrap();
    Mock::given(method("DELETE"))
        .and(path("/notifications/threads/9000"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&env.server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/notifications/threads/9001"))
        .respond_with(ResponseTemplate::new(422).set_body_json(json!({ "message": "Validation Failed" })))
        .expect(2)
        .mount(&env.server)
        .await;

    // 9999 is gone (sync dropped it): left out, the others go in.
    let batch = env.engine.enqueue_all(vec![done("9000"), done("9999"), done("9001")]).await.unwrap();
    assert_eq!(batch.ids.len(), 2);
    assert_eq!(batch.failed, 1);
    assert!(batch.first_error.as_deref().unwrap().contains("not found"), "{batch:?}");
    assert_eq!(env.q::<i64>("SELECT count(*) FROM action_queue"), 2);
    assert_eq!(env.q::<i64>("SELECT sum(done) FROM notifications"), 2);
    // One write, one change event.
    assert_eq!(env.emitted.lock().unwrap().iter().filter(|s| *s == "inbox").count(), 1);

    assert_eq!(env.engine.process_actions().await.unwrap(), 2);
    assert_eq!(env.engine.status().await.unwrap().failed_actions, 1);
    assert_eq!(env.q::<i64>("SELECT done FROM notifications WHERE id = '9001'"), 0);

    // Retry all re-applies the change and queues it again.
    env.engine.retry_all().await.unwrap();
    assert_eq!(env.q::<i64>("SELECT done FROM notifications WHERE id = '9001'"), 1);
    assert_eq!(env.q::<String>("SELECT state FROM action_queue"), "pending");
    env.engine.process_actions().await.unwrap();
    assert_eq!(env.engine.status().await.unwrap().failed_actions, 1);

    env.engine.discard_all().await.unwrap();
    assert_eq!(env.q::<i64>("SELECT count(*) FROM action_queue"), 0);
    assert_eq!(env.q::<i64>("SELECT done FROM notifications WHERE id = '9001'"), 0);
}

/// Bench `optimistic_action_network_wait_ms`: enqueue + reading the
/// optimistic row must not wait on the network, even while the worker is
/// blocked on a slow GitHub answer.
#[tokio::test(flavor = "multi_thread")]
async fn optimistic_apply_does_not_wait_on_network() {
    let env = common::env().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": 1 })).set_delay(Duration::from_secs(3)))
        .mount(&env.server)
        .await;
    env.engine.enqueue(Action::Comment { issue_id: 1, body_md: "slow".into() }).await.unwrap();
    let engine = env.engine.clone();
    let worker = tokio::spawn(async move { engine.process_actions().await });
    tokio::time::sleep(Duration::from_millis(100)).await;

    let t = Instant::now();
    env.engine.enqueue(close(1)).await.unwrap();
    let state: String = env.q("SELECT state FROM issues WHERE id = 1");
    let ms = t.elapsed().as_millis();
    assert_eq!(state, "closed");
    println!("optimistic_action_network_wait_ms = {ms}");
    assert!(ms < 500, "enqueue waited {ms} ms");
    assert!(!worker.is_finished());
    worker.abort();
}
