use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use crabbit_common::SyncResult;
use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(sync_all))
        .route("/:id", post(sync_one))
}

/// Sync all enabled repos from GitHub into the local task queue.
async fn sync_all(State(s): State<AppState>) -> ApiResult<SyncResult> {
    let (token, repos) = get_token_and_repos(&s)?;
    let gh = crate::github::GitHubClient::from_token(token);
    let mut total = SyncResult { created: 0, updated: 0, closed: 0 };

    for repo in repos {
        // Fetch issues async without holding the DB lock
        let issues = match crate::github::fetch_issues_for_sync(&gh, &repo).await {
            Ok(issues) => issues,
            Err(e) if e.to_string().contains("GITHUB_AUTH_EXPIRED") => {
                s.with_db(crate::db::auth::expire_github_token)?;
                return Err(ApiError::BadRequest("GitHub token expired".into()));
            }
            Err(e) => {
                tracing::warn!("sync fetch failed for {}/{}: {e}", repo.owner, repo.name);
                continue;
            }
        };
        // Write to DB synchronously (no await while lock is held)
        match s.with_db(|c| crate::github::sync_issues_to_db(c, &repo, &issues)) {
            Ok(r) => total.merge(r),
            Err(e) => tracing::warn!("sync write failed for {}/{}: {e}", repo.owner, repo.name),
        }
    }

    Ok(Json(total))
}

/// Sync a single repo by id.
async fn sync_one(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(StatusCode, Json<SyncResult>), ApiError> {
    let repo = s.with_db(|c| crate::db::repos::get_repo(c, id))?
        .ok_or_else(|| ApiError::NotFound("repo".into()))?;

    if !repo.enabled {
        return Err(ApiError::BadRequest("repo is not enabled".into()));
    }

    let (token, _) = get_token_and_repos(&s)?;
    let gh = crate::github::GitHubClient::from_token(token);

    let issues = match crate::github::fetch_issues_for_sync(&gh, &repo).await {
        Ok(issues) => issues,
        Err(e) if e.to_string().contains("GITHUB_AUTH_EXPIRED") => {
            s.with_db(crate::db::auth::expire_github_token)?;
            return Err(ApiError::BadRequest("GitHub token expired".into()));
        }
        Err(e) => return Err(ApiError::Internal(e)),
    };

    let result = s.with_db(|c| crate::github::sync_issues_to_db(c, &repo, &issues))?;
    Ok((StatusCode::OK, Json(result)))
}

fn get_token_and_repos(s: &AppState) -> Result<(String, Vec<crabbit_common::Repo>), ApiError> {
    let encrypted_token = s.with_db(crate::db::auth::get_github_token)?;
    let token = match encrypted_token {
        None => return Err(ApiError::BadRequest("GitHub not connected".into())),
        Some(enc) => {
            let key = s.config.encryption_key().map_err(ApiError::Internal)?;
            crate::crypto::decrypt(&enc, &key).map_err(ApiError::Internal)?
        }
    };
    let repos = s.with_db(crate::db::repos::list_enabled_repos)?;
    Ok((token, repos))
}
