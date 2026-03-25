use axum::{routing::get, Json, Router};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/github/status", get(status))
}

async fn status() -> Json<serde_json::Value> {
    Json(serde_json::json!({"connected": false}))
}
