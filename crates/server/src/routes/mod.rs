use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json, Response},
    Router,
};
use serde_json::json;
use crate::state::AppState;

pub mod agent;
pub mod auth;
pub mod repos;
pub mod settings;
pub mod tasks;

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .nest("/repos", repos::router())
        .nest("/tasks", tasks::router())
        .nest("/agent", agent::router())
        .nest("/auth", auth::router())
        .nest("/claude-settings", settings::router())
        .layer(middleware::from_fn_with_state(state.clone(), require_api_key))
        .with_state(state.clone());

    Router::new()
        .nest("/api/v1", api)
        .fallback(crate::embed::serve_static)
        .with_state(state)
}

async fn require_api_key(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let provided = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match provided {
        Some(key) if key == state.config.api_key => next.run(req).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "unauthorized"})),
        )
            .into_response(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use axum::http::{HeaderName, HeaderValue};
    use axum_test::TestServer;
    use crate::{
        config::{Config, GitHubOAuthConfig},
        db::open_db,
        state::AppState,
    };

    pub fn test_config() -> Config {
        Config {
            bind: "127.0.0.1:0".into(),
            db_path: ":memory:".into(),
            api_key: "test".into(),
            encryption_key_hex: "a".repeat(64),
            github_oauth: GitHubOAuthConfig {
                client_id: "id".into(),
                client_secret: "sec".into(),
                callback_url_override: None,
            },
        }
    }

    pub fn test_state() -> AppState {
        let conn = open_db(":memory:").unwrap();
        AppState::new(conn, test_config())
    }

    pub fn test_server() -> TestServer {
        TestServer::new(build_router(test_state())).unwrap()
    }

    pub trait AddAuth {
        fn add_auth(self) -> Self;
    }

    impl AddAuth for axum_test::TestRequest {
        fn add_auth(self) -> Self {
            self.add_header(
                HeaderName::from_static("authorization"),
                HeaderValue::from_static("Bearer test"),
            )
        }
    }

    #[tokio::test]
    async fn unauthenticated_request_returns_401() {
        let server = TestServer::new(build_router(test_state())).unwrap();
        let response = server.get("/api/v1/repos").await;
        assert_eq!(response.status_code(), 401);
    }

    #[tokio::test]
    async fn authenticated_request_reaches_handler() {
        let server = test_server();
        let response = server
            .get("/api/v1/repos")
            .add_auth()
            .await;
        assert_eq!(response.status_code(), 200);
    }
}
