use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, patch, post},
    Json, Router,
};
use crabbit_common::{CreateRepoRequest, Repo, UpdateRepoRequest};
use crate::{db::repos as db, error::{ApiError, ApiResult}, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", patch(update).delete(remove))
}

async fn list(State(s): State<AppState>) -> ApiResult<Vec<Repo>> {
    let repos = s.with_db(|c| db::list_repos(c))?;
    Ok(Json(repos))
}

async fn create(
    State(s): State<AppState>,
    Json(req): Json<CreateRepoRequest>,
) -> Result<(StatusCode, Json<Repo>), ApiError> {
    let now = unix_now();
    let id = s.with_db(|c| db::insert_repo(c, &req.owner, &req.name, req.label_filter.as_deref(), now))?;
    let repo = s.with_db(|c| db::get_repo(c, id))?.ok_or_else(|| ApiError::Internal(anyhow::anyhow!("inserted but not found")))?;
    Ok((StatusCode::CREATED, Json(repo)))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateRepoRequest>,
) -> Result<Json<Repo>, ApiError> {
    s.with_db(|c| db::get_repo(c, id))?.ok_or_else(|| ApiError::NotFound("repo".into()))?;
    if let Some(enabled) = req.enabled {
        s.with_db(|c| db::set_repo_enabled(c, id, enabled))?;
    }
    if let Some(label) = req.label_filter {
        s.with_db(|c| db::set_repo_label_filter(c, id, label.as_deref()))?;
    }
    let repo = s.with_db(|c| db::get_repo(c, id))?.unwrap();
    Ok(Json(repo))
}

async fn remove(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    s.with_db(|c| db::get_repo(c, id))?.ok_or_else(|| ApiError::NotFound("repo".into()))?;
    s.with_db(|c| db::delete_repo(c, id))?;
    Ok(StatusCode::NO_CONTENT)
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
    use crabbit_common::Repo;
    use crate::routes::tests::{test_server, AddAuth};

    #[tokio::test]
    async fn list_repos_empty() {
        let server = test_server();
        let r = server.get("/api/v1/repos").add_auth().await;
        assert_eq!(r.status_code(), 200);
        let repos: Vec<Repo> = r.json();
        assert!(repos.is_empty());
    }

    #[tokio::test]
    async fn create_and_list_repo() {
        let server = test_server();
        let r = server
            .post("/api/v1/repos")
            .add_auth()
            .json(&serde_json::json!({"owner": "acme", "name": "api"}))
            .await;
        assert_eq!(r.status_code(), 201);
        let created: Repo = r.json();
        assert_eq!(created.owner, "acme");

        let r2 = server.get("/api/v1/repos").add_auth().await;
        let repos: Vec<Repo> = r2.json();
        assert_eq!(repos.len(), 1);
    }

    #[tokio::test]
    async fn delete_repo_test() {
        let server = test_server();
        let r = server.post("/api/v1/repos").add_auth()
            .json(&serde_json::json!({"owner": "x", "name": "y"})).await;
        let repo: Repo = r.json();
        let r2 = server.delete(&format!("/api/v1/repos/{}", repo.id)).add_auth().await;
        assert_eq!(r2.status_code(), 204);
    }
}
