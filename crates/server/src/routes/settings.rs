use axum::{routing::get, Json, Router};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(get_settings))
}

async fn get_settings() -> Json<serde_json::Value> {
    Json(serde_json::json!({}))
}
