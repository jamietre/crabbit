use axum::http::{HeaderName, HeaderValue};
use axum_test::TestServer;
use crabbit_server::{
    config::{Config, GitHubOAuthConfig},
    db::open_db,
    routes::build_router,
    state::AppState,
};

fn make_server() -> TestServer {
    let conn = open_db(":memory:").unwrap();
    let config = Config {
        bind: "127.0.0.1:0".into(),
        db_path: ":memory:".into(),
        api_key: "test".into(),
        encryption_key_hex: "a".repeat(64),
        github_oauth: GitHubOAuthConfig {
            client_id: "id".into(),
            client_secret: "sec".into(),
            callback_url_override: None,
        },
    };
    let state = AppState::new(conn, config);
    TestServer::new(build_router(state)).unwrap()
}

fn auth_header() -> (HeaderName, HeaderValue) {
    (
        HeaderName::from_static("authorization"),
        HeaderValue::from_static("Bearer test"),
    )
}

#[tokio::test]
async fn full_lifecycle() {
    let server = make_server();

    // 1. Create repo
    let (k, v) = auth_header();
    let r = server.post("/api/v1/repos").add_header(k, v)
        .json(&serde_json::json!({"owner": "acme", "name": "api"})).await;
    assert_eq!(r.status_code(), 201);
    let repo: serde_json::Value = r.json();
    let repo_id = repo["id"].as_i64().unwrap();

    // 2. Create task
    let (k, v) = auth_header();
    let r = server.post("/api/v1/tasks").add_header(k, v)
        .json(&serde_json::json!({
            "repo_id": repo_id, "issue_number": 1,
            "issue_title": "Fix login", "issue_url": "https://gh/1", "issue_body": "broken"
        })).await;
    assert_eq!(r.status_code(), 201);
    let task: serde_json::Value = r.json();
    let task_id = task["id"].as_i64().unwrap();
    assert_eq!(task["status"], "pending");

    // 3. Mark in progress
    let (k, v) = auth_header();
    server.patch(&format!("/api/v1/tasks/{}", task_id)).add_header(k, v)
        .json(&serde_json::json!({"status": "in_progress"})).await;

    // 4. Post event
    let (k, v) = auth_header();
    server.post(&format!("/api/v1/tasks/{}/events", task_id)).add_header(k, v)
        .json(&serde_json::json!({"event_type": "claude_output", "payload": {"text": "Analyzing..."}}))
        .await;

    // 5. Mark pr_created
    let (k, v) = auth_header();
    server.patch(&format!("/api/v1/tasks/{}", task_id)).add_header(k, v)
        .json(&serde_json::json!({"status": "pr_created", "pr_url": "https://gh/pull/2", "pr_number": 2}))
        .await;

    // 6. Get task with events
    let (k, v) = auth_header();
    let r = server.get(&format!("/api/v1/tasks/{}", task_id)).add_header(k, v).await;
    let full: serde_json::Value = r.json();
    assert_eq!(full["status"], "pr_created");
    assert_eq!(full["pr_number"], 2);
    assert_eq!(full["events"].as_array().unwrap().len(), 1);

    // 7. Agent state shows idle
    let (k, v) = auth_header();
    let r = server.get("/api/v1/agent/state").add_header(k, v).await;
    let state: serde_json::Value = r.json();
    assert_eq!(state["status"], "idle");
}
