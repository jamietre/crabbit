use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use crabbit_common::{ClaudeAuthStatus, PushClaudeAuthRequest};
use crate::{
    db::claude_auth as db,
    error::{ApiError, ApiResult},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_status))
        .route("/token", get(get_token))
        .route("/", put(push_token).delete(clear_token))
}

async fn get_status(State(s): State<AppState>) -> ApiResult<ClaudeAuthStatus> {
    let status = s.with_db(db::get_claude_auth_status)?;
    Ok(Json(status))
}

/// Returns the decrypted credentials JSON for use by the orchestrator.
async fn get_token(State(s): State<AppState>) -> ApiResult<serde_json::Value> {
    let encrypted = s.with_db(db::get_claude_oauth_token)?;
    let creds = match encrypted {
        None => return Ok(Json(serde_json::json!({ "credentials_json": null }))),
        Some(enc) => {
            let key = s.config.encryption_key().map_err(ApiError::Internal)?;
            crate::crypto::decrypt(&enc, &key).map_err(ApiError::Internal)?
        }
    };
    Ok(Json(serde_json::json!({ "credentials_json": creds })))
}

/// Accepts a token push from the desktop sync daemon.
/// Validates the sync_secret, then encrypts and stores the token.
async fn push_token(
    State(s): State<AppState>,
    Json(req): Json<PushClaudeAuthRequest>,
) -> Result<StatusCode, ApiError> {
    // Validate sync secret
    match &s.config.claude_sync_secret {
        None => return Err(ApiError::BadRequest(
            "claude_sync_secret not configured on server — set it in server.toml".into()
        )),
        Some(expected) if expected != &req.sync_secret => {
            return Err(ApiError::Forbidden);
        }
        _ => {}
    }

    let key = s.config.encryption_key().map_err(ApiError::Internal)?;
    let encrypted = crate::crypto::encrypt(&req.credentials_json, &key)
        .map_err(ApiError::Internal)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    s.with_db(|c| db::set_claude_oauth_token(c, &encrypted, now))?;
    tracing::info!("Claude OAuth token updated via sync daemon");
    Ok(StatusCode::NO_CONTENT)
}

async fn clear_token(State(s): State<AppState>) -> Result<StatusCode, ApiError> {
    s.with_db(db::clear_claude_oauth_token)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use crate::routes::tests::{test_server, test_state};
    use axum_test::TestServer;
    use crate::routes::build_router;

    #[tokio::test]
    async fn status_not_configured_initially() {
        let server = test_server();
        let r = server.get("/api/v1/claude-auth/status").await;
        let s: serde_json::Value = r.json();
        assert_eq!(s["configured"], false);
    }

    #[tokio::test]
    async fn push_without_secret_configured_returns_400() {
        let server = test_server();
        let r = server.put("/api/v1/claude-auth")
            .json(&serde_json::json!({"credentials_json": "{}", "sync_secret": "any"}))
            .await;
        assert_eq!(r.status_code(), 400);
    }

    #[tokio::test]
    async fn push_with_wrong_secret_returns_403() {
        let mut state = test_state();
        state.config = std::sync::Arc::new({
            let mut c = (*state.config).clone();
            c.claude_sync_secret = Some("correct".into());
            c
        });
        let server = TestServer::new(build_router(state)).unwrap();
        let r = server.put("/api/v1/claude-auth")
            .json(&serde_json::json!({"credentials_json": "{}", "sync_secret": "wrong"}))
            .await;
        assert_eq!(r.status_code(), 403);
    }

    #[tokio::test]
    async fn push_stores_token_and_status_shows_configured() {
        let mut state = test_state();
        state.config = std::sync::Arc::new({
            let mut c = (*state.config).clone();
            c.claude_sync_secret = Some("s3cret".into());
            c
        });
        let server = TestServer::new(build_router(state)).unwrap();
        server.put("/api/v1/claude-auth")
            .json(&serde_json::json!({"credentials_json": "{\"claudeAiOauth\":{\"accessToken\":\"my_token\"}}", "sync_secret": "s3cret"}))
            .await;
        let r = server.get("/api/v1/claude-auth/status").await;
        let s: serde_json::Value = r.json();
        assert_eq!(s["configured"], true);
    }
}
