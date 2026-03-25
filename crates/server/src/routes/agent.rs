use axum::{
    extract::State,
    routing::{get, put},
    Json, Router,
};
use crabbit_common::{AgentState, UpdateAgentStateRequest};
use crate::{
    db::agent as db,
    error::{ApiError, ApiResult},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/state", get(get_state).put(update_state))
        .route("/next-issue", get(next_issue))
}

async fn get_state(State(s): State<AppState>) -> ApiResult<AgentState> {
    let state = s.with_db(|c| db::get_agent_state(c))?;
    Ok(Json(state))
}

async fn update_state(
    State(s): State<AppState>,
    Json(req): Json<UpdateAgentStateRequest>,
) -> ApiResult<AgentState> {
    s.with_db(|c| db::set_agent_state(c, &req))?;
    let state = s.with_db(|c| db::get_agent_state(c))?;
    Ok(Json(state))
}

async fn next_issue() -> Json<Option<serde_json::Value>> {
    Json(None) // Stubbed — will be implemented in Task 16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::tests::{test_server, AddAuth};

    #[tokio::test]
    async fn agent_state_default_is_idle() {
        let server = test_server();
        let r = server.get("/api/v1/agent/state").add_auth().await;
        let state: serde_json::Value = r.json();
        assert_eq!(state["status"], "idle");
    }

    #[tokio::test]
    async fn set_agent_sleeping() {
        let server = test_server();
        server.put("/api/v1/agent/state").add_auth()
            .json(&serde_json::json!({"status": "sleeping", "wake_at": 9999999999i64}))
            .await;
        let r = server.get("/api/v1/agent/state").add_auth().await;
        let state: serde_json::Value = r.json();
        assert_eq!(state["status"], "sleeping");
        assert_eq!(state["wake_at"], 9999999999i64);
    }
}
