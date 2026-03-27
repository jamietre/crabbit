use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use crabbit_common::{ClaudeAuthStatus, PushClaudeAuthRequest};
use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_status))
        .route("/", put(push_token).delete(clear_token))
}

async fn get_status(State(s): State<AppState>) -> ApiResult<ClaudeAuthStatus> {
    let path = match &s.config.claude_credentials_path {
        None => return Ok(Json(ClaudeAuthStatus { configured: false, updated_at: None })),
        Some(p) => std::path::PathBuf::from(p),
    };
    match std::fs::metadata(&path) {
        Ok(meta) => {
            let updated_at = meta.modified().ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            Ok(Json(ClaudeAuthStatus { configured: true, updated_at }))
        }
        Err(_) => Ok(Json(ClaudeAuthStatus { configured: false, updated_at: None })),
    }
}

async fn push_token(
    State(s): State<AppState>,
    Json(req): Json<PushClaudeAuthRequest>,
) -> Result<StatusCode, ApiError> {
    match &s.config.claude_sync_secret {
        None => return Err(ApiError::BadRequest(
            "claude_sync_secret not configured on server — set it in server.toml".into()
        )),
        Some(expected) if expected != &req.sync_secret => {
            return Err(ApiError::Forbidden);
        }
        _ => {}
    }

    let path = s.config.claude_credentials_path.as_deref()
        .ok_or_else(|| ApiError::BadRequest(
            "claude_credentials_path not configured in server.toml".into()
        ))?;

    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| ApiError::Internal(anyhow::anyhow!("cannot create credentials dir: {}", e)))?;
    }

    std::fs::write(path, req.credentials_json.as_bytes())
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("cannot write credentials file: {}", e)))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }

    tracing::info!("Claude credentials written to {}", path);
    Ok(StatusCode::NO_CONTENT)
}

async fn clear_token(State(s): State<AppState>) -> Result<StatusCode, ApiError> {
    if let Some(path) = &s.config.claude_credentials_path {
        let _ = std::fs::remove_file(path);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use crate::routes::tests::{test_server, test_state};
    use axum_test::TestServer;
    use crate::routes::build_router;
    use tempfile::tempdir;

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
        let dir = tempdir().unwrap();
        let creds_path = dir.path().join(".credentials.json");
        let mut state = test_state();
        state.config = std::sync::Arc::new({
            let mut c = (*state.config).clone();
            c.claude_sync_secret = Some("correct".into());
            c.claude_credentials_path = Some(creds_path.to_str().unwrap().into());
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
        let dir = tempdir().unwrap();
        let creds_path = dir.path().join(".credentials.json");
        let mut state = test_state();
        state.config = std::sync::Arc::new({
            let mut c = (*state.config).clone();
            c.claude_sync_secret = Some("s3cret".into());
            c.claude_credentials_path = Some(creds_path.to_str().unwrap().into());
            c
        });
        let server = TestServer::new(build_router(state)).unwrap();
        server.put("/api/v1/claude-auth")
            .json(&serde_json::json!({"credentials_json": "{\"claudeAiOauth\":{\"accessToken\":\"tok\"}}", "sync_secret": "s3cret"}))
            .await;
        assert!(creds_path.exists());
        let r = server.get("/api/v1/claude-auth/status").await;
        let s: serde_json::Value = r.json();
        assert_eq!(s["configured"], true);
    }
}
