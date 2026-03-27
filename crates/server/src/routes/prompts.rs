use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use crabbit_common::{CreatePromptRequest, Prompt, UpdatePromptRequest};
use crate::{db::prompts as db, error::{ApiError, ApiResult}, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/:id", get(get_one).put(update).delete(remove))
}

#[derive(serde::Deserialize)]
struct ListPromptsQuery {
    category: Option<String>,
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListPromptsQuery>,
) -> ApiResult<Vec<Prompt>> {
    let prompts = if let Some(cat) = q.category {
        s.with_db(move |c| db::list_prompts_by_category(c, &cat))?
    } else {
        s.with_db(db::list_prompts)?
    };
    Ok(Json(prompts))
}

async fn get_one(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Prompt>, ApiError> {
    let prompt = s.with_db(|c| db::get_prompt(c, id))?
        .ok_or_else(|| ApiError::NotFound("prompt".into()))?;
    Ok(Json(prompt))
}

async fn create(
    State(s): State<AppState>,
    Json(req): Json<CreatePromptRequest>,
) -> Result<(StatusCode, Json<Prompt>), ApiError> {
    let now = unix_now();
    let label = req.label.as_deref().unwrap_or("");
    let id = s.with_db(|c| db::insert_prompt(c, &req.category, label, &req.name, &req.content, now))?;
    let prompt = s.with_db(|c| db::get_prompt(c, id))?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("inserted but not found")))?;
    Ok((StatusCode::CREATED, Json(prompt)))
}

async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<UpdatePromptRequest>,
) -> Result<Json<Prompt>, ApiError> {
    s.with_db(|c| db::get_prompt(c, id))?
        .ok_or_else(|| ApiError::NotFound("prompt".into()))?;
    let now = unix_now();
    s.with_db(|c| {
        db::update_prompt(
            c,
            id,
            req.category.as_deref(),
            req.label.as_deref(),
            req.name.as_deref(),
            req.content.as_deref(),
            req.enabled,
            now,
        )
    })?;
    let prompt = s.with_db(|c| db::get_prompt(c, id))?.unwrap();
    Ok(Json(prompt))
}

async fn remove(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    s.with_db(|c| db::get_prompt(c, id))?
        .ok_or_else(|| ApiError::NotFound("prompt".into()))?;
    s.with_db(|c| db::delete_prompt(c, id))?;
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
    use crabbit_common::Prompt;
    use crate::routes::tests::test_server;

    #[tokio::test]
    async fn list_prompts_returns_seeds() {
        let server = test_server();
        let r = server.get("/api/v1/prompts").await;
        assert_eq!(r.status_code(), 200);
        let prompts: Vec<Prompt> = r.json();
        // Seed data is inserted on DB open
        assert!(!prompts.is_empty());
        assert!(prompts.iter().any(|p| p.category == "triage"));
        assert!(prompts.iter().any(|p| p.category == "code"));
    }

    #[tokio::test]
    async fn create_and_list_prompt() {
        let server = test_server();
        let before: Vec<Prompt> = server.get("/api/v1/prompts").await.json();
        let r = server
            .post("/api/v1/prompts")
            .json(&serde_json::json!({
                "category": "triage",
                "label": "",
                "name": "Extra triage prompt",
                "content": "Triage the issue and assess complexity."
            }))
            .await;
        assert_eq!(r.status_code(), 201);
        let created: Prompt = r.json();
        assert_eq!(created.category, "triage");
        assert_eq!(created.name, "Extra triage prompt");
        assert!(created.enabled);

        let r2 = server.get("/api/v1/prompts").await;
        let prompts: Vec<Prompt> = r2.json();
        assert_eq!(prompts.len(), before.len() + 1);
    }

    #[tokio::test]
    async fn update_prompt() {
        let server = test_server();
        let r = server
            .post("/api/v1/prompts")
            .json(&serde_json::json!({
                "category": "code",
                "name": "Code guidance",
                "content": "Write clean code."
            }))
            .await;
        let created: Prompt = r.json();

        let r2 = server
            .put(&format!("/api/v1/prompts/{}", created.id))
            .json(&serde_json::json!({ "enabled": false }))
            .await;
        assert_eq!(r2.status_code(), 200);
        let updated: Prompt = r2.json();
        assert!(!updated.enabled);
    }

    #[tokio::test]
    async fn delete_prompt_test() {
        let server = test_server();
        let r = server
            .post("/api/v1/prompts")
            .json(&serde_json::json!({
                "category": "plan",
                "name": "Plan prompt",
                "content": "Plan the implementation."
            }))
            .await;
        let created: Prompt = r.json();
        let r2 = server.delete(&format!("/api/v1/prompts/{}", created.id)).await;
        assert_eq!(r2.status_code(), 204);
    }

    #[tokio::test]
    async fn get_nonexistent_prompt_returns_404() {
        let server = test_server();
        let r = server.get("/api/v1/prompts/999").await;
        assert_eq!(r.status_code(), 404);
    }
}
