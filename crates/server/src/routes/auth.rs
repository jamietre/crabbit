use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Redirect,
    routing::{delete, get},
    Json, Router,
};
use crabbit_common::GitHubAuthStatus;
use serde::Deserialize;
use uuid::Uuid;
use crate::{
    db::auth as db,
    error::{ApiError, ApiResult},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/github/status", get(status))
        .route("/github/begin", get(begin))
        .route("/github/callback", get(callback))
        .route("/github", delete(disconnect))
}

#[derive(Deserialize)]
struct StatusQuery {
    include_token: Option<bool>,
}

async fn status(
    State(s): State<AppState>,
    Query(q): Query<StatusQuery>,
) -> ApiResult<GitHubAuthStatus> {
    let mut auth = s.with_db(|c| db::get_github_auth_status(c))?;
    if q.include_token.unwrap_or(false) && auth.connected {
        // Decrypt and expose the token for the orchestrator
        if let Ok(Some(enc)) = s.with_db(|c| db::get_github_token(c)) {
            if let Ok(key) = s.config.encryption_key() {
                auth.access_token = crate::crypto::decrypt(&enc, &key).ok();
            }
        }
    }
    Ok(Json(auth))
}

async fn begin(State(s): State<AppState>) -> ApiResult<serde_json::Value> {
    let state_nonce = Uuid::new_v4().to_string();
    let expiry = unix_now() + 600;
    {
        let mut map = s.pending_oauth.lock().map_err(|_| ApiError::Internal(anyhow::anyhow!("lock poisoned")))?;
        map.insert(state_nonce.clone(), expiry);
    }
    let url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&scope=repo&state={}",
        s.config.github_oauth.client_id,
        state_nonce
    );
    Ok(Json(serde_json::json!({ "url": url })))
}

#[derive(Deserialize)]
struct CallbackQuery {
    code: String,
    state: String,
}

async fn callback(
    State(s): State<AppState>,
    Query(q): Query<CallbackQuery>,
) -> Result<Redirect, ApiError> {
    // Validate state nonce
    {
        let mut map = s.pending_oauth.lock().map_err(|_| ApiError::Internal(anyhow::anyhow!("lock poisoned")))?;
        let expiry = map.remove(&q.state).ok_or_else(|| ApiError::BadRequest("invalid state".into()))?;
        if unix_now() > expiry {
            return Err(ApiError::BadRequest("state expired".into()));
        }
    }

    // Exchange code for token
    let client = reqwest::Client::new();
    let token_resp: serde_json::Value = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id": s.config.github_oauth.client_id,
            "client_secret": s.config.github_oauth.client_secret,
            "code": q.code,
        }))
        .send().await
        .map_err(|e| ApiError::Internal(e.into()))?
        .json().await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let access_token = token_resp["access_token"]
        .as_str()
        .ok_or_else(|| ApiError::BadRequest("no access_token in response".into()))?
        .to_string();

    let scopes = token_resp["scope"].as_str().unwrap_or("").to_string();

    // Fetch GitHub user
    let user: serde_json::Value = client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", access_token))
        .header("User-Agent", "crabbit/1.0")
        .send().await
        .map_err(|e| ApiError::Internal(e.into()))?
        .json().await
        .map_err(|e| ApiError::Internal(e.into()))?;

    let login = user["login"].as_str().unwrap_or("unknown").to_string();

    // Encrypt and store
    let key = s.config.encryption_key().map_err(|e| ApiError::Internal(e))?;
    let encrypted = crate::crypto::encrypt(&access_token, &key).map_err(|e| ApiError::Internal(e))?;
    s.with_db(|c| db::set_github_auth(c, &encrypted, &scopes, &login, unix_now()))?;

    Ok(Redirect::to("/"))
}

async fn disconnect(State(s): State<AppState>) -> Result<StatusCode, ApiError> {
    s.with_db(|c| db::clear_github_auth(c))?;
    Ok(StatusCode::NO_CONTENT)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::tests::{test_server, test_state, AddAuth};
    use axum_test::TestServer;
    use crate::routes::build_router;

    #[tokio::test]
    async fn auth_status_not_connected_initially() {
        let server = test_server();
        let r = server.get("/api/v1/auth/github/status").add_auth().await;
        let s: serde_json::Value = r.json();
        assert_eq!(s["connected"], false);
    }

    #[tokio::test]
    async fn begin_returns_github_url() {
        let server = test_server();
        let r = server.get("/api/v1/auth/github/begin").add_auth().await;
        let s: serde_json::Value = r.json();
        let url = s["url"].as_str().unwrap();
        assert!(url.starts_with("https://github.com/login/oauth/authorize"));
        assert!(url.contains("client_id=id"));
        assert!(url.contains("scope=repo"));
    }

    #[tokio::test]
    async fn disconnect_clears_status() {
        let state = test_state();
        state.with_db(|c| crate::db::auth::set_github_auth(c, "enc_token", "repo", "testuser", 1_700_000_000)).unwrap();
        let server = TestServer::new(build_router(state)).unwrap();
        let r = server.get("/api/v1/auth/github/status").add_auth().await;
        let s: serde_json::Value = r.json();
        assert_eq!(s["connected"], true);
        server.delete("/api/v1/auth/github").add_auth().await;
        let r2 = server.get("/api/v1/auth/github/status").add_auth().await;
        let s2: serde_json::Value = r2.json();
        assert_eq!(s2["connected"], false);
    }
}
