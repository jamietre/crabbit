use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post, put},
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

async fn next_issue(State(s): State<AppState>) -> Result<Json<Option<NextIssueResponse>>, ApiError> {
    use crabbit_common::TaskStatus;

    // Get GitHub token
    let encrypted_token = s.with_db(|c| crate::db::auth::get_github_token(c))?;
    let token = match encrypted_token {
        None => return Ok(Json(None)),
        Some(enc) => {
            let key = s.config.encryption_key()
                .map_err(|e| ApiError::Internal(e))?;
            crate::crypto::decrypt(&enc, &key)
                .map_err(|e| ApiError::Internal(e))?
        }
    };

    let repos = s.with_db(|c| crate::db::repos::list_enabled_repos(c))?;
    if repos.is_empty() {
        return Ok(Json(None));
    }

    let gh = crate::github::GitHubClient::from_token(token);

    for repo in repos {
        let issues = gh
            .list_open_issues(&repo.owner, &repo.name, repo.label_filter.as_deref())
            .await
            .map_err(|e| ApiError::Internal(e))?;

        for issue in issues {
            let existing = s.with_db(|c| {
                crate::db::tasks::get_task_by_issue(c, repo.id, issue.number)
            })?;
            match existing {
                None => {
                    return Ok(Json(Some(NextIssueResponse {
                        repo_id: repo.id,
                        repo_owner: repo.owner,
                        repo_name: repo.name,
                        issue_number: issue.number,
                        issue_title: issue.title,
                        issue_url: issue.html_url,
                        issue_body: issue.body,
                        existing_task_id: None,
                    })));
                }
                Some(t) if t.status == TaskStatus::Pending => {
                    return Ok(Json(Some(NextIssueResponse {
                        repo_id: repo.id,
                        repo_owner: repo.owner,
                        repo_name: repo.name,
                        issue_number: issue.number,
                        issue_title: issue.title,
                        issue_url: issue.html_url,
                        issue_body: issue.body,
                        existing_task_id: Some(t.id),
                    })));
                }
                Some(_) => continue,
            }
        }
    }

    Ok(Json(None))
}

async fn trigger_run(State(s): State<AppState>) -> Result<impl axum::response::IntoResponse, ApiError> {
    let agent_state = s.with_db(|c| db::get_agent_state(c))?;
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

    #[tokio::test]
    async fn next_issue_returns_null_when_no_repos() {
        let server = test_server();
        let r = server.get("/api/v1/agent/next-issue").add_auth().await;
        assert_eq!(r.status_code(), 200);
        let body: serde_json::Value = r.json();
        assert!(body.is_null());
    }
}
