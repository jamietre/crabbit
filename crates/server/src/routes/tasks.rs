use axum::{routing::get, Json, Router};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list_tasks))
}

async fn list_tasks() -> Json<Vec<serde_json::Value>> {
    Json(vec![])
}
