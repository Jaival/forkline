mod common;

use fl_types::{Action, AuthPoll};
use serde_json::{Value, json};
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, ResponseTemplate};

fn repo_json(id: i64, name: &str) -> Value {
    json!({
        "id": id, "node_id": format!("R_{id}"), "name": name, "owner": { "login": "o", "id": 10 },
        "description": "d", "language": "Rust", "default_branch": "main", "license": { "spdx_id": "MIT" },
        "private": false, "fork": false, "stargazers_count": 7, "forks_count": 1,
        "pushed_at": "2024-01-01T00:00:00Z", "updated_at": "2024-01-02T00:00:00Z"
    })
}

/// No deep sync, no notifications, viewer known: only `repos` runs.
async fn only_repos(env: &common::Env) {
    env.db
        .write(|c| {
            c.execute_batch(
                "INSERT INTO sync_state (key, last_synced_at, poll_interval) VALUES ('notifications', 4102444800, 60);",
            )
        })
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn repo_metadata_pages_starred_and_etag() {
    let env = common::env().await;
    only_repos(&env).await;
    let page2 = format!("{}/user/repos?page=2", env.server.uri());
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .and(header("If-None-Match", "\"e1\""))
        .respond_with(ResponseTemplate::new(304))
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .and(wiremock::matchers::query_param("page", "2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo_json(1, "r")])))
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"e1\"")
                .insert_header("link", format!("<{page2}>; rel=\"next\"").as_str())
                .set_body_json(json!([repo_json(3000, "new")])),
        )
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user/starred"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo_json(4000, "starred-elsewhere")])))
        .mount(&env.server)
        .await;

    env.engine.sync_once().await;
    assert_eq!(env.engine.status().await.unwrap().error, None);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM repos"), 4);
    assert_eq!(env.q::<i64>("SELECT stars FROM repos WHERE id = 1"), 7);
    assert_eq!(env.q::<String>("SELECT license FROM repos WHERE id = 3000"), "MIT");
    assert_eq!(env.q::<i64>("SELECT starred FROM repos WHERE id = 4000"), 1);
    assert_eq!(env.q::<i64>("SELECT sum(starred) FROM repos"), 1);
    assert_eq!(env.q::<String>("SELECT etag FROM sync_state WHERE key = 'repos'"), "\"e1\"");
    assert!(env.emitted.lock().unwrap().contains(&"repos".to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn notifications_poll_and_respect_pending_fields() {
    let env = common::env().await;
    env.db
        .write(|c| c.execute_batch("INSERT INTO sync_state (key, last_synced_at) VALUES ('repos', 4102444800);"))
        .unwrap();
    // Marked read locally; GitHub still says unread.
    env.engine.enqueue(Action::NotificationRead { id: "9000".into() }).await.unwrap();
    let thread = |id: &str, n: i64| {
        json!({
            "id": id, "unread": true, "reason": "review_requested", "updated_at": "2024-03-01T00:00:00Z",
            "subject": { "title": format!("T{n}"), "type": "Issue", "url": format!("https://api.github.com/repos/o/r/issues/{n}") },
            "repository": { "id": 1, "node_id": "R1", "name": "r", "owner": { "login": "o" } }
        })
    };
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-poll-interval", "120")
                .insert_header("last-modified", "Fri, 01 Mar 2024 00:00:00 GMT")
                .set_body_json(json!([thread("9000", 1), thread("9001", 2)])),
        )
        .mount(&env.server)
        .await;

    env.engine.sync_once().await;
    assert_eq!(env.engine.status().await.unwrap().error, None);
    assert_eq!(env.q::<i64>("SELECT unread FROM notifications WHERE id = '9000'"), 0);
    assert_eq!(env.q::<i64>("SELECT unread FROM notifications WHERE id = '9001'"), 1);
    assert_eq!(env.q::<i64>("SELECT issue_id FROM notifications WHERE id = '9001'"), 2);
    assert_eq!(env.q::<i64>("SELECT poll_interval FROM sync_state WHERE key = 'notifications'"), 120);
    assert!(env.emitted.lock().unwrap().contains(&"inbox".to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn rate_limit_pauses_sync() {
    let env = common::env().await;
    env.db
        .write(|c| c.execute_batch("INSERT INTO sync_state (key, last_synced_at) VALUES ('repos', 4102444800);"))
        .unwrap();
    let reset = fl_sync::now() + 600;
    Mock::given(method("GET"))
        .and(path("/notifications"))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", reset.to_string().as_str())
                .set_body_json(json!({ "message": "API rate limit exceeded" })),
        )
        .expect(1)
        .mount(&env.server)
        .await;
    env.engine.sync_once().await;
    let s = env.engine.status().await.unwrap();
    assert!(s.paused);
    assert!(s.error.unwrap().contains("10 min"));
    // Paused: the next pass makes no request (expect(1) above).
    env.engine.sync_once().await;
}

fn actor(id: i64, login: &str) -> Value {
    json!({ "login": login, "databaseId": id, "avatarUrl": null, "name": null })
}

fn issue_node(id: i64, number: i64, updated: &str) -> Value {
    json!({
        "id": format!("I_{id}"), "databaseId": id, "number": number, "title": format!("Title {number}"),
        "body": "body", "bodyHTML": "<p>body</p>", "state": "OPEN",
        "createdAt": "2024-01-01T00:00:00Z", "updatedAt": updated, "closedAt": null,
        "author": actor(11, "alice"), "milestone": null,
        "labels": { "nodes": [{ "name": "docs" }] },
        "assignees": { "nodes": [actor(12, "bob")] },
        "comments": { "totalCount": 1, "nodes": [{
            "databaseId": id * 10, "body": "c", "bodyHTML": "<p>c</p>",
            "createdAt": "2024-01-01T00:00:00Z", "updatedAt": "2024-01-01T00:00:00Z", "author": actor(11, "alice")
        }]}
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn deep_sync_writes_issues_prs_and_keeps_pending_fields() {
    let env = common::env().await;
    env.db
        .write(|c| {
            c.execute_batch(
                "UPDATE repos SET pinned = 1 WHERE id = 1;
                 INSERT INTO sync_state (key, last_synced_at) VALUES ('repos', 4102444800);
                 INSERT INTO sync_state (key, last_synced_at, poll_interval) VALUES ('notifications', 4102444800, 60);",
            )
        })
        .unwrap();
    // Local close of issue 1 is pending; sync must keep it closed.
    env.engine.enqueue(Action::SetState { issue_id: 1, state: fl_types::IssueState::Closed }).await.unwrap();

    Mock::given(method("GET"))
        .and(path("/repos/o/r/labels"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": 100, "name": "bug", "color": "f00" }, { "id": 101, "name": "docs", "color": "0f0" }
        ])))
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/repos/o/r/contributors"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("languages(first"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": { "repository": {
            "languages": { "edges": [{ "size": 99, "node": { "name": "Rust" } }] },
            "releases": { "nodes": [{ "databaseId": 7, "tagName": "v1", "name": "One", "publishedAt": "2024-01-01T00:00:00Z" }] },
            "defaultBranchRef": { "target": { "history": { "nodes": [{
                "oid": "c1", "messageHeadline": "init", "authoredDate": "2024-01-01T00:00:00Z",
                "author": { "user": { "login": "alice" } }, "statusCheckRollup": { "state": "SUCCESS" }
            }]}}}
        }}})))
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("issues(first"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": { "repository": { "issues": {
            "pageInfo": { "hasNextPage": false, "endCursor": "x" },
            "nodes": [issue_node(1, 1, "2024-02-01T00:00:00Z"), issue_node(3, 3, "2024-02-02T00:00:00Z")]
        }}}})))
        .mount(&env.server)
        .await;
    let mut pr = issue_node(2, 2, "2024-02-03T00:00:00Z");
    let extra = json!({
        "state": "OPEN", "isDraft": true, "mergeable": "MERGEABLE", "mergeStateStatus": "CLEAN",
        "reviewDecision": "APPROVED", "baseRefName": "main", "headRefName": "feat",
        "baseRefOid": "b1", "headRefOid": "h1", "additions": 5, "deletions": 2, "changedFiles": 1,
        "reviewRequests": { "nodes": [{ "requestedReviewer": { "login": "me" } }, { "requestedReviewer": { "combinedSlug": "o/team" } }] },
        "closingIssuesReferences": { "nodes": [{ "databaseId": 3 }] },
        "reviews": { "nodes": [{ "databaseId": 70, "state": "APPROVED", "bodyHTML": "", "submittedAt": "2024-02-03T00:00:00Z", "author": actor(11, "alice") }] },
        "reviewThreads": { "totalCount": 1, "nodes": [{
            "id": "T_new", "isResolved": true, "isOutdated": false, "path": "src/b.rs", "diffSide": "RIGHT",
            "line": 9, "startLine": null, "originalLine": 9,
            "comments": { "nodes": [{ "databaseId": 80, "body": "nit", "bodyHTML": "<p>nit</p>", "createdAt": "2024-02-03T00:00:00Z",
                "author": actor(11, "alice"), "commit": { "oid": "h1" }, "pullRequestReview": { "databaseId": 70 } }] }
        }]},
        "commits": { "nodes": [{ "commit": { "oid": "h1", "messageHeadline": "feat", "authoredDate": "2024-02-03T00:00:00Z",
            "author": { "user": { "login": "alice" } }, "statusCheckRollup": { "state": "FAILURE" } } }] },
        "head": { "nodes": [{ "commit": { "oid": "h1", "statusCheckRollup": { "state": "FAILURE", "contexts": { "nodes": [
            { "databaseId": 90, "name": "ci", "status": "COMPLETED", "conclusion": "FAILURE", "detailsUrl": "u",
              "startedAt": "2024-02-03T00:00:00Z", "completedAt": "2024-02-03T00:01:00Z" },
            {}
        ]}}}}]}
    });
    pr.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("pullRequests(first"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": { "repository": { "pullRequests": {
            "pageInfo": { "hasNextPage": false, "endCursor": "y" }, "nodes": [pr]
        }}}})))
        .mount(&env.server)
        .await;

    env.engine.sync_once().await;
    assert_eq!(env.engine.status().await.unwrap().error, None);

    // Issues, with the pending local state kept.
    assert_eq!(env.q::<String>("SELECT state FROM issues WHERE id = 1"), "closed");
    assert_eq!(env.q::<String>("SELECT title FROM issues WHERE id = 1"), "Title 1");
    assert_eq!(env.q::<i64>("SELECT count(*) FROM issues WHERE repo_id = 1"), 3);
    assert_eq!(env.q::<i64>("SELECT label_id FROM issue_labels WHERE issue_id = 3"), 101);
    assert_eq!(env.q::<String>("SELECT u.login FROM issue_assignees a JOIN users u ON u.id = a.user_id WHERE a.issue_id = 3"), "bob");
    assert_eq!(env.q::<i64>("SELECT count(*) FROM comments WHERE issue_id = 3"), 1);
    // PR details and children.
    assert_eq!(env.q::<String>("SELECT head_oid FROM pr_details WHERE issue_id = 2"), "h1");
    assert_eq!(env.q::<String>("SELECT check_state FROM pr_details WHERE issue_id = 2"), "FAILURE");
    assert_eq!(env.q::<i64>("SELECT count(*) FROM pr_review_requests WHERE pr_id = 2"), 2);
    assert_eq!(env.q::<i64>("SELECT issue_id FROM issue_links WHERE pr_id = 2"), 3);
    assert_eq!(env.q::<i64>("SELECT review_id FROM review_comments WHERE id = 80"), 70);
    // The old thread 50 is gone (complete list); the new one is resolved.
    assert_eq!(env.q::<i64>("SELECT count(*) FROM review_threads WHERE pr_id = 2"), 1);
    assert_eq!(env.q::<i64>("SELECT is_resolved FROM review_threads WHERE id = 80"), 1);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM checks"), 1);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM pr_commits WHERE pr_id = 2"), 1);
    // Overview.
    assert_eq!(env.q::<String>("SELECT languages_json FROM repos WHERE id = 1"), r#"[{"bytes":99,"name":"Rust"}]"#);
    assert_eq!(env.q::<i64>("SELECT on_default_branch FROM commits WHERE oid = 'c1'"), 1);
    assert_eq!(env.q::<String>("SELECT tag FROM releases"), "v1");
    // Cursors for the next incremental run.
    assert_eq!(env.q::<String>("SELECT cursor FROM sync_state WHERE key = 'repo:1:issues'"), "2024-02-02T00:00:00Z");
    assert_eq!(env.q::<String>("SELECT cursor FROM sync_state WHERE key = 'repo:1:pulls'"), "2024-02-03T00:00:00Z");
    assert!(env.q::<i64>("SELECT last_synced_at FROM sync_state WHERE key = 'repo:1'") > 0);
    assert!(env.emitted.lock().unwrap().contains(&"pr".to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn device_flow_signs_in_and_writes_viewer() {
    let env = common::env().await;
    env.engine.set_test_token(None);
    env.db.write(|c| c.execute("DELETE FROM settings", [])).unwrap();
    assert!(!env.engine.auth_state().await.unwrap().signed_in);

    Mock::given(method("POST"))
        .and(path("/login/device/code"))
        .and(body_string_contains("client_id=test-client"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "dc", "user_code": "ABCD-1234", "verification_uri": "https://github.com/login/device",
            "expires_in": 900, "interval": 5
        })))
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "error": "slow_down", "interval": 10 })))
        .up_to_n_times(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/login/oauth/access_token"))
        .and(body_string_contains("device_code=dc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "gho_new", "token_type": "bearer" })))
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user"))
        .and(header("authorization", "Bearer gho_new"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": 42, "login": "octo" })))
        .mount(&env.server)
        .await;

    let code = env.engine.auth_start().await.unwrap();
    assert_eq!(code.user_code, "ABCD-1234");
    assert!(matches!(env.engine.auth_poll().await.unwrap(), AuthPoll::Pending { interval: 10 }));
    match env.engine.auth_poll().await.unwrap() {
        AuthPoll::Done { login } => assert_eq!(login, "octo"),
        other => panic!("{other:?}"),
    }
    let state = env.engine.auth_state().await.unwrap();
    assert!(state.signed_in && state.client_id_configured && !state.env_override);
    assert_eq!(state.login.as_deref(), Some("octo"));
    assert_eq!(env.q::<String>("SELECT value_json FROM settings WHERE key = 'viewer'"), r#"{"id":42,"login":"octo"}"#);

    env.engine.sign_out().await.unwrap();
    assert!(!env.engine.auth_state().await.unwrap().signed_in);
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_client_id_explains_the_dev_override() {
    let env = common::env().await;
    let mut cfg = fl_sync::Config::test(&env.server.uri());
    cfg.client_id = None;
    let engine = fl_sync::Engine::new(env.db.clone(), cfg);
    assert!(!engine.auth_state().await.unwrap().client_id_configured);
    let err = engine.auth_start().await.unwrap_err().to_string();
    assert!(err.contains("FORKLINE_GITHUB_TOKEN"), "{err}");
}

// ---- paging, timeline, contributors, star history ----------------------------

/// Pinned repo 1, everything else fresh; mocks labels, overview, and an empty PR list.
async fn deep_env(contributors: ResponseTemplate) -> common::Env {
    let env = common::env().await;
    env.db
        .write(|c| {
            c.execute_batch(
                "UPDATE repos SET pinned = 1 WHERE id = 1;
                 INSERT INTO sync_state (key, last_synced_at) VALUES ('repos', 4102444800);
                 INSERT INTO sync_state (key, last_synced_at, poll_interval) VALUES ('notifications', 4102444800, 60);",
            )
        })
        .unwrap();
    Mock::given(method("GET")).and(path("/repos/o/r/labels")).respond_with(ResponseTemplate::new(200).set_body_json(json!([]))).mount(&env.server).await;
    Mock::given(method("GET")).and(path("/repos/o/r/contributors")).respond_with(contributors).mount(&env.server).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("languages(first"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": { "repository": {} } })))
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("pullRequests(first"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": { "repository": { "pullRequests": {
            "pageInfo": { "hasNextPage": false, "endCursor": null }, "nodes": []
        }}}})))
        .mount(&env.server)
        .await;
    env
}

async fn mount_issues(env: &common::Env, node: Value) {
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("issues(first"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": { "repository": { "issues": {
            "pageInfo": { "hasNextPage": false, "endCursor": null }, "nodes": [node]
        }}}})))
        .mount(&env.server)
        .await;
}

fn comment(id: i64) -> Value {
    json!({ "databaseId": id, "body": format!("c{id}"), "bodyHTML": "<p>c</p>",
        "createdAt": "2024-01-01T00:00:00Z", "updatedAt": "2024-01-01T00:00:00Z", "author": actor(11, "alice") })
}

#[tokio::test(flavor = "multi_thread")]
async fn comments_beyond_one_page_are_fetched_with_node_queries() {
    let env = deep_env(ResponseTemplate::new(204)).await;
    let mut node = issue_node(3, 3, "2024-02-01T00:00:00Z");
    node["comments"] = json!({ "totalCount": 3,
        "pageInfo": { "hasPreviousPage": true, "startCursor": "cur1" }, "nodes": [comment(32)] });
    mount_issues(&env, node).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("node(id:$id)"))
        .and(body_string_contains("\"c\":\"cur1\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": { "node": { "comments": {
            "pageInfo": { "hasPreviousPage": false, "startCursor": "cur0" }, "nodes": [comment(30), comment(31)]
        }}}})))
        .expect(1)
        .mount(&env.server)
        .await;

    env.engine.sync_once().await;
    assert_eq!(env.engine.status().await.unwrap().error, None);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM comments WHERE issue_id = 3"), 3);
    assert_eq!(env.q::<String>("SELECT group_concat(id) FROM (SELECT id FROM comments WHERE issue_id = 3 ORDER BY id)"), "30,31,32");
}

#[tokio::test(flavor = "multi_thread")]
async fn timeline_events_are_written_and_replaced() {
    let env = deep_env(ResponseTemplate::new(204)).await;
    env.db
        .write(|c| {
            c.execute_batch(
                "INSERT INTO issues (id, node_id, repo_id, number, is_pr, state, title, created_at, updated_at) VALUES (3, 'I_3', 1, 3, 0, 'open', 'x', 0, 0);
                 INSERT INTO timeline_events (issue_id, kind, created_at) VALUES (3, 'closed', 1);",
            )
        })
        .unwrap();
    let mut node = issue_node(3, 3, "2024-02-01T00:00:00Z");
    let at = "2024-01-05T00:00:00Z";
    node["timelineItems"] = json!({ "pageInfo": { "hasNextPage": true, "endCursor": "t1" }, "nodes": [
        { "__typename": "LabeledEvent", "createdAt": at, "actor": actor(11, "alice"), "label": { "name": "bug" } },
        { "__typename": "AssignedEvent", "createdAt": at, "actor": actor(11, "alice"), "assignee": { "login": "bob" } },
    ]});
    mount_issues(&env, node).await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_string_contains("node(id:$id)"))
        .and(body_string_contains("\"c\":\"t1\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": { "node": { "timelineItems": {
            "pageInfo": { "hasNextPage": false, "endCursor": "t2" }, "nodes": [
                { "__typename": "CrossReferencedEvent", "createdAt": at, "actor": null,
                  "source": { "number": 9, "repository": { "nameWithOwner": "o/other" } } },
                { "__typename": "RenamedTitleEvent", "createdAt": at, "actor": actor(11, "alice"), "previousTitle": "a", "currentTitle": "b" },
                { "__typename": "ClosedEvent", "createdAt": at, "actor": actor(11, "alice") },
            ]
        }}}})))
        .mount(&env.server)
        .await;

    env.engine.sync_once().await;
    assert_eq!(env.engine.status().await.unwrap().error, None);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM timeline_events WHERE issue_id = 3"), 5);
    assert_eq!(env.q::<String>("SELECT payload_json FROM timeline_events WHERE kind = 'labeled'"), r#"{"label":"bug"}"#);
    assert_eq!(env.q::<String>("SELECT payload_json FROM timeline_events WHERE kind = 'assigned'"), r#"{"assignee":"bob"}"#);
    assert_eq!(env.q::<String>("SELECT payload_json FROM timeline_events WHERE kind = 'referenced'"), r##"{"ref":"o/other#9"}"##);
    assert_eq!(env.q::<String>("SELECT payload_json FROM timeline_events WHERE kind = 'renamed'"), r#"{"from":"a","to":"b"}"#);
    assert_eq!(env.q::<Option<String>>("SELECT payload_json FROM timeline_events WHERE kind = 'closed'"), None);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM timeline_events WHERE created_at = 1"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn contributors_are_replaced_and_empty_repo_204_clears_them() {
    let env = deep_env(ResponseTemplate::new(200).set_body_json(json!([
        { "id": 11, "login": "alice", "avatar_url": "a", "contributions": 40 },
        { "id": 12, "login": "bob", "avatar_url": "b", "contributions": 3 },
        { "type": "Anonymous", "name": "x", "contributions": 1 },
    ])))
    .await;
    mount_issues(&env, issue_node(3, 3, "2024-02-01T00:00:00Z")).await;
    env.engine.sync_once().await;
    assert_eq!(env.engine.status().await.unwrap().error, None);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM repo_contributors WHERE repo_id = 1"), 2);
    assert_eq!(env.q::<i64>("SELECT contributions FROM repo_contributors WHERE user_id = 11"), 40);

    // Same flow with a 204 (empty repo): rows are replaced by nothing.
    let env = deep_env(ResponseTemplate::new(204)).await;
    env.db
        .write(|c| c.execute_batch("INSERT INTO repo_contributors (repo_id, user_id, contributions) VALUES (1, 11, 5);"))
        .unwrap();
    mount_issues(&env, issue_node(3, 3, "2024-02-01T00:00:00Z")).await;
    env.engine.sync_once().await;
    assert_eq!(env.engine.status().await.unwrap().error, None);
    assert_eq!(env.q::<i64>("SELECT count(*) FROM repo_contributors"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn star_history_gets_one_snapshot_per_day() {
    let env = common::env().await;
    only_repos(&env).await;
    Mock::given(method("GET"))
        .and(path("/user/repos"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([repo_json(1, "r")])))
        .mount(&env.server)
        .await;
    Mock::given(method("GET")).and(path("/user/starred")).respond_with(ResponseTemplate::new(200).set_body_json(json!([]))).mount(&env.server).await;
    env.engine.sync_once().await;
    assert_eq!(env.engine.status().await.unwrap().error, None);
    assert_eq!(env.q::<i64>("SELECT stars FROM star_history WHERE repo_id = 1"), 7);
    assert_eq!(env.q::<i64>("SELECT day FROM star_history WHERE repo_id = 1"), fl_sync::now() / 86400);
    // Repo 2 was not part of the response: no snapshot.
    assert_eq!(env.q::<i64>("SELECT count(*) FROM star_history WHERE repo_id = 2"), 0);
}
