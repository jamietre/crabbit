use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, patch},
    Json, Router,
};
use crabbit_common::{
    CreateTaskEventRequest, CreateTaskRequest, ListTasksQuery,
    Task, TaskEvent, TaskStatus, TaskWithEvents, UpdateTaskRequest,
};
use crate::{
    db::{repos as repos_db, tasks as db},
    error::{ApiError, ApiResult},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", get(get_one).patch(update))
        .route("/:id/events", post(add_event))
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListTasksQuery>,
) -> ApiResult<Vec<Task>> {
    let status = q.status.as_deref().and_then(|s| s.parse::<TaskStatus>().ok());
    let tasks = s.with_db(|c| db::list_tasks(c, status.as_ref(), q.repo_id, q.limit.unwrap_or(100), q.offset.unwrap_or(0)))?;
    Ok(Json(tasks))
}

async fn get_one(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<TaskWithEvents> {
    let task = s.with_db(|c| db::get_task(c, id))?.ok_or_else(|| ApiError::NotFound("task".into()))?;
    let events = s.with_db(|c| db::list_task_events(c, id))?;
    Ok(Json(TaskWithEvents { task, events }))
}

async fn create(
    State(s): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<Task>), ApiError> {
    // Validate repo exists
    let repo = s.with_db(|c| repos_db::get_repo(c, req.repo_id))?;
    if repo.is_none() {
        return Err(ApiError::BadRequest(format!("repo {} not found", req.repo_id)));
    }
    let now = unix_now();
    let id = s.with_db(|c| db::insert_task(c, req.repo_id, req.issue_number, &req.issue_title, &req.issue_url, &req.issue_body, now))?;
    let task = s.with_db(|c| db::get_task(c, id))?.ok_or_else(|| ApiError::Internal(anyhow::anyhow!("inserted but not found")))?;
    Ok((StatusCode::CREATED, Json(task)))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateTaskRequest>,
) -> ApiResult<Task> {
    s.with_db(|c| db::get_task(c, id))?.ok_or_else(|| ApiError::NotFound("task".into()))?;
    let now = unix_now();
    if let Some(ref status) = req.status {
        s.with_db(|c| db::update_task_status(c, id, status, now))?;
    }
    if req.pr_url.is_some() || req.pr_number.is_some() || req.error_message.is_some() || req.claude_session_id.is_some() {
        let task = s.with_db(|c| db::get_task(c, id))?.unwrap();
        let status = req.status.as_ref().unwrap_or(&task.status);
        s.with_db(|c| db::update_task_outcome(
            c, id, status,
            req.pr_url.as_deref().or(task.pr_url.as_deref()),
            req.pr_number.or(task.pr_number),
            req.error_message.as_deref().or(task.error_message.as_deref()),
            req.claude_session_id.as_deref().or(task.claude_session_id.as_deref()),
            now,
        ))?;
    }
    let task = s.with_db(|c| db::get_task(c, id))?.unwrap();
    Ok(Json(task))
}

async fn add_event(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<CreateTaskEventRequest>,
) -> Result<(StatusCode, Json<TaskEvent>), ApiError> {
    s.with_db(|c| db::get_task(c, id))?.ok_or_else(|| ApiError::NotFound("task".into()))?;
    let now = unix_now();
    let event_id = s.with_db(|c| db::insert_task_event(c, id, &req.event_type, &req.payload, now))?;
    let events = s.with_db(|c| db::list_task_events(c, id))?;
    let event = events.into_iter().find(|e| e.id == event_id)
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("event not found after insert")))?;
    Ok((StatusCode::CREATED, Json(event)))
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::tests::{test_server, AddAuth};

    #[tokio::test]
    async fn create_task_requires_valid_repo() {
        let server = test_server();
        let r = server.post("/api/v1/tasks").add_auth()
            .json(&serde_json::json!({
                "repo_id": 9999, "issue_number": 1,
                "issue_title": "t", "issue_url": "u", "issue_body": "b"
            })).await;
        assert_eq!(r.status_code(), 400);
    }

    #[tokio::test]
    async fn create_and_get_task_with_events() {
        let server = test_server();
        let repo: serde_json::Value = server.post("/api/v1/repos").add_auth()
            .json(&serde_json::json!({"owner": "x", "name": "y"})).await.json();
        let repo_id = repo["id"].as_i64().unwrap();

        let r = server.post("/api/v1/tasks").add_auth()
            .json(&serde_json::json!({
                "repo_id": repo_id, "issue_number": 5,
                "issue_title": "Fix it", "issue_url": "https://gh/5", "issue_body": "body"
            })).await;
        assert_eq!(r.status_code(), 201);
        let task: serde_json::Value = r.json();
        let task_id = task["id"].as_i64().unwrap();

        server.post(&format!("/api/v1/tasks/{}/events", task_id)).add_auth()
            .json(&serde_json::json!({"event_type": "status_change", "payload": {"from": "pending"}}))
            .await;

        let r2 = server.get(&format!("/api/v1/tasks/{}", task_id)).add_auth().await;
        let full: serde_json::Value = r2.json();
        assert_eq!(full["events"].as_array().unwrap().len(), 1);
    }
}
