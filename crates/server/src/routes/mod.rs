use axum::Router;
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
        .with_state(state.clone());

    Router::new()
        .nest("/api/v1", api)
        .fallback(crate::embed::serve_static)
        .with_state(state)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
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
            encryption_key_hex: "a".repeat(64),
            github_oauth: GitHubOAuthConfig {
                client_id: "id".into(),
                client_secret: "sec".into(),
                callback_url_override: None,
            },
            orchestrator_script: "/dev/null".into(),
            agent_env: "/dev/null".into(),
        }
    }

    pub fn test_state() -> AppState {
        let conn = open_db(":memory:").unwrap();
        AppState::new(conn, test_config())
    }

    pub fn test_server() -> TestServer {
        TestServer::new(build_router(test_state())).unwrap()
    }

    #[tokio::test]
    async fn request_reaches_handler() {
        let server = test_server();
        let response = server.get("/api/v1/repos").await;
        assert_eq!(response.status_code(), 200);
    }
}
