use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use crabbit_common::{AgentState, AgentStatus, NextIssueResponse, UpdateAgentStateRequest};
use crate::{
    db::agent as db,
    error::{ApiError, ApiResult},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/state", get(get_state).put(update_state))
        .route("/next-issue", get(next_issue))
        .route("/run", post(trigger_run))
}

async fn get_state(State(s): State<AppState>) -> ApiResult<AgentState> {
    let state = s.with_db(db::get_agent_state)?;
    Ok(Json(state))
}

async fn update_state(
    State(s): State<AppState>,
    Json(req): Json<UpdateAgentStateRequest>,
) -> ApiResult<AgentState> {
    s.with_db(|c| db::set_agent_state(c, &req))?;
    let state = s.with_db(db::get_agent_state)?;
    Ok(Json(state))
}

async fn next_issue(State(s): State<AppState>) -> Result<Json<Option<NextIssueResponse>>, ApiError> {
    let task = s.with_db(crate::db::tasks::next_queued_task)?;
    match task {
        None => Ok(Json(None)),
        Some(t) => {
            let repo = s.with_db(|c| crate::db::repos::get_repo(c, t.repo_id))?
                .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("task has missing repo {}", t.repo_id)))?;
            Ok(Json(Some(NextIssueResponse {
                task_id: t.id,
                repo_id: repo.id,
                repo_owner: repo.owner,
                repo_name: repo.name,
                issue_number: t.issue_number,
                issue_title: t.issue_title,
                issue_url: t.issue_url,
                issue_body: t.issue_body,
                completion_prompt: repo.completion_prompt,
                existing_task_id: Some(t.id),
            })))
        }
    }
}

async fn trigger_run(State(s): State<AppState>) -> Result<impl axum::response::IntoResponse, ApiError> {
    let agent_state = s.with_db(db::get_agent_state)?;
    if agent_state.status == AgentStatus::Running {
        return Err(ApiError::BadRequest("agent is already running".into()));
    }

    let script = crate::expand_tilde(std::path::Path::new(&s.config.orchestrator_script));
    let env_path = crate::expand_tilde(std::path::Path::new(&s.config.agent_env));

    tokio::process::Command::new(&script)
        .env("CRABBIT_CONFIG", &env_path)
        .spawn()
        .map_err(|e| ApiError::Internal(anyhow::anyhow!(
            "failed to spawn {}: {e}", script.display()
        )))?;

    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({"spawned": true}))))
}

#[cfg(test)]
mod tests {
    use crate::routes::tests::test_server;

    #[tokio::test]
    async fn agent_state_default_is_idle() {
        let server = test_server();
        let r = server.get("/api/v1/agent/state").await;
        let state: serde_json::Value = r.json();
        assert_eq!(state["status"], "idle");
    }

    #[tokio::test]
    async fn set_agent_sleeping() {
        let server = test_server();
        server.put("/api/v1/agent/state")
            .json(&serde_json::json!({"status": "sleeping", "wake_at": 9999999999i64}))
            .await;
        let r = server.get("/api/v1/agent/state").await;
        let state: serde_json::Value = r.json();
        assert_eq!(state["status"], "sleeping");
        assert_eq!(state["wake_at"], 9999999999i64);
    }

    #[tokio::test]
    async fn next_issue_returns_null_when_no_repos() {
        let server = test_server();
        let r = server.get("/api/v1/agent/next-issue").await;
        assert_eq!(r.status_code(), 200);
        let body: serde_json::Value = r.json();
        assert!(body.is_null());
    }
}
