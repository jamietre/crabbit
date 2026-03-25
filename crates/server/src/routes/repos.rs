use axum::{routing::get, Json, Router};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list_repos))
}

async fn list_repos() -> Json<Vec<serde_json::Value>> {
    Json(vec![])
}
