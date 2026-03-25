use axum::{routing::get, Json, Router};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/state", get(get_state))
        .route("/next-issue", get(next_issue))
}

async fn get_state() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "idle"}))
}

async fn next_issue() -> Json<Option<serde_json::Value>> {
    Json(None)
}
