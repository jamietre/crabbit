use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post, put},
    Json, Router,
};
use crabbit_common::{ClaudeAuthCheckStatus, ClaudeAuthStatus, PushClaudeAuthRequest, UpdateAgentStateRequest};
use crate::{
    db::{agent as agent_db, claude_auth_check as check_db},
    error::{ApiError, ApiResult},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_status))
        .route("/check", post(trigger_check))
        .route("/", put(push_token).delete(clear_token))
}

async fn get_status(State(s): State<AppState>) -> ApiResult<ClaudeAuthStatus> {
    let path = match &s.config.claude_credentials_path {
        None => return Ok(Json(ClaudeAuthStatus {
            configured: false,
            updated_at: None,
            check: s.with_db(check_db::get)?,
        })),
        Some(p) => std::path::PathBuf::from(p),
    };
    let (configured, updated_at) = match std::fs::metadata(&path) {
        Ok(meta) => {
            let updated_at = meta.modified().ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            (true, updated_at)
        }
        Err(_) => (false, None),
    };
    Ok(Json(ClaudeAuthStatus {
        configured,
        updated_at,
        check: s.with_db(check_db::get)?,
    }))
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

    // Kick off a background auth check after a sync so status stays current.
    let state = s.clone();
    tokio::spawn(async move {
        if let Err(e) = run_check(&state).await {
            tracing::warn!("post-sync auth check failed: {}", e);
        }
    });

    Ok(StatusCode::NO_CONTENT)
}

async fn clear_token(State(s): State<AppState>) -> Result<StatusCode, ApiError> {
    if let Some(path) = &s.config.claude_credentials_path {
        let _ = std::fs::remove_file(path);
    }
    let now = unix_now();
    s.with_db(|c| check_db::set(c, "unknown", now, None))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn trigger_check(State(s): State<AppState>) -> ApiResult<ClaudeAuthCheckStatus> {
    let result = run_check(&s).await?;
    Ok(Json(result))
}

/// Reads the credentials file, calls the Anthropic usage endpoint, and persists the result.
pub async fn run_check(s: &AppState) -> anyhow::Result<ClaudeAuthCheckStatus> {
    let now = unix_now();

    let creds_path = match &s.config.claude_credentials_path {
        None => {
            let result = ClaudeAuthCheckStatus {
                status: "unknown".into(),
                checked_at: Some(now),
                error: Some("claude_credentials_path not configured".into()),
            };
            s.with_db(|c| check_db::set(c, &result.status, now, result.error.as_deref()))?;
            return Ok(result);
        }
        Some(p) => p.clone(),
    };

    let creds_json = match std::fs::read_to_string(&creds_path) {
        Ok(s) => s,
        Err(e) => {
            let msg = format!("cannot read credentials file: {}", e);
            s.with_db(|c| check_db::set(c, "unknown", now, Some(&msg)))?;
            return Ok(ClaudeAuthCheckStatus {
                status: "unknown".into(),
                checked_at: Some(now),
                error: Some(msg),
            });
        }
    };

    let token = match extract_access_token(&creds_json) {
        Some(t) => t,
        None => {
            let msg = "credentials file missing claudeAiOauth.accessToken".to_string();
            s.with_db(|c| check_db::set(c, "unknown", now, Some(&msg)))?;
            return Ok(ClaudeAuthCheckStatus {
                status: "unknown".into(),
                checked_at: Some(now),
                error: Some(msg),
            });
        }
    };

    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", format!("Bearer {}", token))
        .header("anthropic-beta", "oauth-2025-04-20")
        .send()
        .await;

    let result = match resp {
        Ok(r) if r.status().is_success() => {
            // Parse usage fields from the response body and update agent state.
            if let Ok(body) = r.json::<serde_json::Value>().await {
                let pct_7d = body["seven_day"]["utilization"].as_f64();
                let pct_5h = body["five_hour"]["utilization"].as_f64();
                let reset_at = body["seven_day"]["resets_at"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.timestamp());
                let usage_req = UpdateAgentStateRequest {
                    status: None, wake_at: None, current_task_id: None, usage_note: None,
                    usage_pct_7d: pct_7d,
                    usage_pct_5h: pct_5h,
                    usage_reset_at: reset_at,
                };
                let _ = s.with_db(|c| agent_db::set_agent_state(c, &usage_req));
            }
            s.with_db(|c| check_db::set(c, "ok", now, None))?;
            ClaudeAuthCheckStatus { status: "ok".into(), checked_at: Some(now), error: None }
        }
        Ok(r) if r.status() == reqwest::StatusCode::UNAUTHORIZED => {
            s.with_db(|c| check_db::set(c, "expired", now, Some("401 Unauthorized")))?;
            ClaudeAuthCheckStatus {
                status: "expired".into(),
                checked_at: Some(now),
                error: Some("401 Unauthorized".into()),
            }
        }
        Ok(r) => {
            let msg = format!("unexpected status {}", r.status());
            s.with_db(|c| check_db::set(c, "unknown", now, Some(&msg)))?;
            ClaudeAuthCheckStatus { status: "unknown".into(), checked_at: Some(now), error: Some(msg) }
        }
        Err(e) => {
            let msg = format!("request failed: {}", e);
            s.with_db(|c| check_db::set(c, "unknown", now, Some(&msg)))?;
            ClaudeAuthCheckStatus { status: "unknown".into(), checked_at: Some(now), error: Some(msg) }
        }
    };

    tracing::info!("Claude auth check: {}", result.status);
    Ok(result)
}

fn extract_access_token(creds_json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(creds_json).ok()?;
    v["claudeAiOauth"]["accessToken"].as_str().map(|s| s.to_string())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
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
        assert_eq!(s["check"]["status"], "unknown");
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
