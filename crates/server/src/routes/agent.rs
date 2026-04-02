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

    // Find the next task to work on
    let task = match s.with_db(crate::db::tasks::next_queued_task)? {
        None => return Ok((StatusCode::OK, Json(serde_json::json!({"status": "no_work"})))),
        Some(t) => t,
    };

    let repo = s.with_db(|c| crate::db::repos::get_repo(c, task.repo_id))?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("task {} has missing repo", task.id)))?;

    // Decrypt GH token
    let enc_token = s.with_db(crate::db::auth::get_github_token)?
        .ok_or_else(|| ApiError::BadRequest("GitHub account not connected".into()))?;
    let key = s.config.encryption_key()
        .map_err(|e| ApiError::Internal(e))?;
    let gh_token = crate::crypto::decrypt(&enc_token, &key)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("failed to decrypt GH token: {e}")))?;

    // Determine Docker image from repo toolchain
    let image = if let Some(tc_name) = &repo.toolchain {
        s.with_db(|c| crate::db::toolchains::get_toolchain(c, tc_name))?
            .map(|tc| tc.image)
            .unwrap_or_else(|| "ghcr.io/jamietre/crabbit-base:latest".into())
    } else {
        "ghcr.io/jamietre/crabbit-base:latest".into()
    };

    // Mark agent as running
    s.with_db(|c| db::set_agent_state(c, &UpdateAgentStateRequest {
        status: Some(AgentStatus::Running),
        current_task_id: Some(task.id),
        wake_at: None,
        usage_note: None,
        usage_pct_7d: None,
        usage_pct_5h: None,
        usage_reset_at: None,
    }))?;

    let claude_config_dir = s.config.claude_config_dir.clone();
    let bind = s.config.bind.clone();
    let api_url = {
        let port = bind.rsplit(':').next().unwrap_or("3000");
        format!("http://localhost:{port}")
    };
    let completion_prompt = repo.completion_prompt.clone().unwrap_or_default();
    let session_id = task.claude_session_id.clone().unwrap_or_default();

    let image_display = image.clone();
    tokio::spawn(async move {
        let mut cmd = tokio::process::Command::new("docker");
        cmd.args([
            "run", "--rm",
            "--network", "host",
            "-e", &format!("GH_TOKEN={gh_token}"),
            "-e", &format!("CRABBIT_API_URL={api_url}"),
            "-e", &format!("CRABBIT_TASK_ID={}", task.id),
            "-e", &format!("CRABBIT_REPO_OWNER={}", repo.owner),
            "-e", &format!("CRABBIT_REPO_NAME={}", repo.name),
            "-e", &format!("CRABBIT_ISSUE_NUMBER={}", task.issue_number),
            "-e", &format!("CRABBIT_ISSUE_TITLE={}", task.issue_title),
            "-e", &format!("CRABBIT_ISSUE_URL={}", task.issue_url),
            "-e", &format!("CRABBIT_ISSUE_BODY={}", task.issue_body),
            "-e", &format!("CRABBIT_COMPLETION_PROMPT={completion_prompt}"),
            "-e", &format!("CRABBIT_SESSION_ID={session_id}"),
            "-v", &format!("{claude_config_dir}:/root/.claude:ro"),
            &image,
        ]);

        let result = cmd.status().await;

        // If docker itself failed (e.g. image not found), mark the task failed
        if result.map(|s| !s.success()).unwrap_or(true) {
            tracing::error!("docker run exited non-zero for task {}", task.id);
        }

        // Reset agent state regardless — the container updates task status via API
        let _ = s.with_db(|c| db::set_agent_state(c, &UpdateAgentStateRequest {
            status: Some(AgentStatus::Idle),
            current_task_id: None,
            wake_at: None,
            usage_note: None,
            usage_pct_7d: None,
            usage_pct_5h: None,
            usage_reset_at: None,
        }));
    });

    Ok((StatusCode::ACCEPTED, Json(serde_json::json!({"spawned": true, "task_id": task.id, "image": image_display}))))
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
