//! Shared test setup: temp DB with one repo, an issue, a PR, labels, users,
//! a review thread and a notification; an engine pointed at a mock server.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use fl_db::Db;
use fl_sync::{Config, Engine};
use wiremock::MockServer;

pub struct Env {
    pub _dir: tempfile::TempDir,
    pub db: Arc<Db>,
    pub server: MockServer,
    pub engine: Arc<Engine>,
    pub emitted: Arc<Mutex<Vec<String>>>,
}

pub const SEED: &str = "
INSERT INTO repos (id, node_id, owner, name, updated_at) VALUES (1, 'R1', 'o', 'r', 0), (2, 'R2', 'o', 'other', 0);
INSERT INTO users (id, login) VALUES (10, 'me'), (11, 'alice');
INSERT INTO settings (key, value_json) VALUES ('viewer', '{\"id\":10,\"login\":\"me\"}');
INSERT INTO labels (id, repo_id, name, color) VALUES (100, 1, 'bug', 'f00'), (101, 1, 'docs', '0f0'), (200, 2, 'x', '000');
INSERT INTO issues (id, node_id, repo_id, number, is_pr, state, title, created_at, updated_at)
  VALUES (1, 'I1', 1, 1, 0, 'open', 'An issue', 0, 0), (2, 'P2', 1, 2, 1, 'open', 'A PR', 0, 0);
INSERT INTO issue_labels (issue_id, label_id) VALUES (1, 100);
INSERT INTO pr_details (issue_id, base_ref, head_ref, base_oid, head_oid) VALUES (2, 'main', 'feat', 'b0', 'h0');
INSERT INTO review_threads (id, node_id, pr_id, path, side, line) VALUES (50, 'T50', 2, 'src/a.rs', 'RIGHT', 3);
INSERT INTO notifications (id, repo_id, subject_type, issue_id, title, reason, unread, updated_at)
  VALUES ('9000', 1, 'Issue', 1, 'An issue', 'mention', 1, 100);
";

pub async fn env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(Db::open(&dir.path().join("t.db")).unwrap());
    db.write(|c| c.execute_batch(SEED)).unwrap();
    let server = MockServer::start().await;
    // Explore runs on every repo tick; follow nobody unless a test says so.
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/user/following"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .with_priority(u8::MAX)
        .mount(&server)
        .await;
    // The viewer's searches run every deep tick; no hits unless a test says so.
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/graphql"))
        .and(wiremock::matchers::body_string_contains("search(query"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({ "data": { "search": {
            "pageInfo": { "hasNextPage": false, "endCursor": null }, "nodes": []
        }}})))
        .with_priority(u8::MAX)
        .mount(&server)
        .await;
    let engine = Engine::new(db.clone(), Config::test(&server.uri()));
    engine.set_test_token(Some("test-token"));
    let emitted = Arc::new(Mutex::new(Vec::new()));
    let e = emitted.clone();
    engine.set_emitter(move |s| e.lock().unwrap().extend(s.iter().map(|s| (*s).to_owned())));
    Env { _dir: dir, db, server, engine, emitted }
}

impl Env {
    pub fn q<T: rusqlite::types::FromSql>(&self, sql: &str) -> T {
        self.db.read(|c| c.query_row(sql, [], |r| r.get(0))).unwrap()
    }
}
