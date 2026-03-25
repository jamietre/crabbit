use axum::{
    extract::State,
    routing::{get, put},
    Json, Router,
};
use crabbit_common::{ClaudeSettings, UpdateClaudeSettingsRequest};
use crate::{
    db::settings as db,
    error::ApiResult,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(get_settings).put(update_settings))
}

async fn get_settings(State(s): State<AppState>) -> ApiResult<ClaudeSettings> {
    let settings = s.with_db(|c| db::get_claude_settings(c))?;
    Ok(Json(settings))
}

async fn update_settings(
    State(s): State<AppState>,
    Json(req): Json<UpdateClaudeSettingsRequest>,
) -> ApiResult<ClaudeSettings> {
    s.with_db(|c| db::update_claude_settings(c, &req))?;
    let settings = s.with_db(|c| db::get_claude_settings(c))?;
    Ok(Json(settings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::tests::{test_server, AddAuth};

    #[tokio::test]
    async fn get_default_settings() {
        let server = test_server();
        let r = server.get("/api/v1/claude-settings").add_auth().await;
        let s: serde_json::Value = r.json();
        assert_eq!(s["model"], "claude-sonnet-4-6");
        assert_eq!(s["effort_level"], "high");
        assert_eq!(s["allow_browser_automation"], true);
    }

    #[tokio::test]
    async fn update_model() {
        let server = test_server();
        server.put("/api/v1/claude-settings").add_auth()
            .json(&serde_json::json!({"model": "claude-opus-4-6"})).await;
        let r = server.get("/api/v1/claude-settings").add_auth().await;
        let s: serde_json::Value = r.json();
        assert_eq!(s["model"], "claude-opus-4-6");
    }
}
